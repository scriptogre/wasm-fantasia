# Rune Scripting System Design

## Goal

Replace hardcoded Rust game logic (combat resolution, crit/buff presets, enemy AI) with Rune scripts. Rune becomes the primary language for gameplay logic — abilities, weapons, passives, enemy behaviors — while Rust owns the engine (ECS, rendering, physics, networking).

## Why Rune

- **Bytecode VM**: Compiles to bytecode, faster than Rhai's AST interpretation
- **Pure Rust**: No C/C++ dependencies, compiles to WASM trivially
- **Rust-like syntax**: Pattern matching, structs, enums, `?` operator — minimal context switching
- **Async-first**: Native async/await for channeled abilities, delayed effects
- **Hot reload**: Change scripts without recompiling during development

Considered alternatives:
- **Rhai**: Pure Rust but AST-interpreted (slower), positioned for config not game logic
- **Luau (mlua)**: Battle-tested (Roblox) and fastest, but C++ dependency makes SpacetimeDB WASM compatibility uncertain
- **bevy_mod_scripting**: Bevy plugin supporting Lua/Rhai/Rune, but adds complexity and may not work with SpacetimeDB

## Architecture

### Execution Model: Both Sides (Roblox-style)

Scripts run on both client and server. Client runs scripts for immediate feel (animations, VFX, sound). Server runs scripts as the authority (damage, health, state changes).

```
┌─────────────────────────────────────────────────┐
│                  core/ crate                     │
│                                                  │
│  ┌────────────┐  ┌─────────────┐  ┌───────────┐ │
│  │ Rune Engine│  │ Script      │  │ Command   │ │
│  │ (compile,  │  │ Registry    │  │ Buffer    │ │
│  │  VM pool)  │  │ (bytecode)  │  │           │ │
│  └─────┬──────┘  └──────┬──────┘  └─────┬─────┘ │
│        └────────────────┼────────────────┘       │
│                         │                        │
│  ┌──────────────────────┴─────────────────────┐  │
│  │         Game Module (the API)              │  │
│  │  reads:  .stats, .pos, .health, ...        │  │
│  │  writes: damage(), heal(), knockback(),    │  │
│  │          vfx(), sound(), animate(),        │  │
│  │          buff(), fire_hook()               │  │
│  └────────────────────────────────────────────┘  │
│                                                  │
│  ┌────────────────────────────────────────────┐  │
│  │            Hook Definitions                │  │
│  │  on_pre_hit, on_hit, on_crit, on_kill,    │  │
│  │  on_tick, on_take_damage,                  │  │
│  │  on_ability_start, on_ability_end,         │  │
│  │  on_buff_applied, on_buff_expired,         │  │
│  │  on_spawn, on_death                        │  │
│  └────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
         │                           │
         ▼                           ▼
┌─────────────────┐       ┌─────────────────┐
│  client/ crate  │       │  server/ crate  │
│                 │       │                 │
│ CommandExecutor │       │ CommandExecutor │
│ (ECS)           │       │ (SpacetimeDB)   │
│                 │       │                 │
│ damage() →      │       │ damage() →      │
│   Health comp   │       │   update table  │
│ vfx() →         │       │ vfx() →         │
│   spawn entity  │       │   no-op/event   │
│ sound() →       │       │ sound() →       │
│   play audio    │       │   no-op         │
│ animate() →     │       │ animate() →     │
│   state machine │       │   no-op         │
└─────────────────┘       └─────────────────┘
```

### Why Both Sides

Server-only would add 50-150ms latency to every ability in multiplayer. For an action RPG, attack animations, VFX, and sound must respond instantly to input. The Roblox model works: client shows the ability immediately, server validates and resolves authoritatively, client reconciles if needed.

Singleplayer with a local SpacetimeDB (~1-5ms RTT) would be fine server-only, but we want one architecture that works for both modes.

### Command Buffer Pattern

Scripts emit commands; they don't mutate game state directly. After a script returns, Rust iterates the command buffer and applies each command using the platform-specific executor.

Reads are direct (scripts access struct fields). Writes are buffered (function calls push commands).

```
Script runs → reads source.stats.crit_chance (direct field access)
            → calls damage(target, 75) (pushes Command::DealDamage to buffer)
            → calls vfx("crit", target.pos) (pushes Command::SpawnVfx to buffer)
Script returns
Rust applies Command::DealDamage → ECS Health component (client) or table update (server)
Rust applies Command::SpawnVfx  → spawn particle entity (client) or no-op (server)
```

Benefits:
- Same scripts, different executors per platform
- Easy to test scripts in isolation (inspect command buffer without a real world)
- Safe — scripts can't corrupt game state mid-execution
- Deterministic — command application order is fixed

### Client vs Server Command Routing

| Command | Client | Server |
|---|---|---|
| `damage(target, amount)` | Modify Health component, spawn damage number | Update table, insert CombatEvent |
| `heal(target, amount)` | Modify Health component, spawn heal number | Update table |
| `knockback(target, force)` | Add PendingKnockback component | Insert KnockbackImpulse row |
| `vfx(name, pos)` | Spawn particle entity | No-op (or insert CombatEvent for remote clients) |
| `sound(name, pos)` | Play audio | No-op |
| `animate(entity, anim)` | Trigger animation state | No-op (or broadcast via position update) |
| `buff(target, name, dur)` | Update local buff tracker, show UI icon | Insert/update ActiveEffect row |
| `set_stat(entity, stat, val)` | Update Stats component | Update table column |

## Script API

### Exposed Types (defined in Rust, visible in Rune)

```
Combatant {
    id: u64,
    pos: Vec3,
    dir: Vec3,
    health: f32,
    max_health: f32,
    // Stats flattened onto combatant for ergonomics:
    attack_damage: f32,
    crit_chance: f32,
    crit_multiplier: f32,
    knockback_force: f32,
    attack_range: f32,
    attack_arc: f32,
    attack_speed: f32,
    // Dynamic stats:
    fury_stacks: i32,
    attack_speed_bonus: f32,
}

Hit {
    damage: f32,
    knockback: f32,
    is_crit: bool,
}
```

Field access on these types is checked at runtime by Rune — accessing a nonexistent field fails immediately with "field not found".

### Module Functions (the `game` module)

```
// Queries
targets_in_cone(source, range, arc) -> Vec<Combatant>
targets_in_radius(pos, radius) -> Vec<Combatant>
distance(a, b) -> f32
chance(probability) -> bool  // deterministic RNG

// Commands (buffered writes)
damage(target, amount)
heal(target, amount)
knockback(target, force)
vfx(name, target_or_pos)
sound(name, pos)
animate(entity, animation_name)
buff(target, name, duration)
remove_buff(target, name)
set_stat(entity, stat_name, value)

// Hook chaining
fire_hook(hook_name, source, target, hit)
```

## Script Organization

```
assets/scripts/
├── behaviors/          # Composable passive effects
│   ├── crit.rune       # on_pre_hit: crit roll + damage multiply
│   ├── stacking.rune   # on_hit: fury stacks, on_buff_expired: reset
│   └── feedback.rune   # on_hit: screen shake, hit stop, sounds
├── abilities/          # Active abilities (attacks, spells)
│   ├── melee_attack.rune
│   └── ground_pound.rune
└── enemies/            # Enemy AI behaviors
    └── zombie_ai.rune  # on_tick: chase/attack decision
```

One script per behavior. Entities get a list of scripts attached. Scripts are composable — a player entity might have `["crit", "stacking", "feedback"]` behaviors and `"melee_attack"` as their active ability.

### Hook Execution Order

When an event fires (e.g., a hit lands), the engine runs the matching hook on each attached behavior in order:

```
Player attacks enemy
→ melee_attack.rune: on_ability_start
  → for each target:
    → builds Hit { damage, knockback, is_crit: false }
    → fire_hook("on_pre_hit") runs:
      → crit.rune: on_pre_hit (may set is_crit, multiply damage)
    → damage(target, hit.damage)
    → knockback(target, hit.knockback)
    → fire_hook("on_hit") runs:
      → stacking.rune: on_hit (add fury stack)
      → feedback.rune: on_hit (screen shake, hit stop)
    → if hit.is_crit: vfx("crit_particles", target.pos)
```

## Example Scripts

### behaviors/crit.rune
```rune
use game::*;

pub fn on_pre_hit(source, target, hit) {
    if chance(source.crit_chance) {
        hit.is_crit = true;
        hit.damage *= source.crit_multiplier;
        hit.knockback *= source.crit_multiplier;
    }
}
```

### behaviors/stacking.rune
```rune
use game::*;

pub fn on_hit(source, target, hit) {
    let add = if hit.is_crit { 3 } else { 1 };
    let stacks = min(source.fury_stacks + add, 12);
    set_stat(source, "fury_stacks", stacks);
    set_stat(source, "attack_speed_bonus", stacks * 0.12);
    buff(source, "fury", 2.5);
}

pub fn on_buff_expired(source, buff_name) {
    if buff_name == "fury" {
        set_stat(source, "fury_stacks", 0);
        set_stat(source, "attack_speed_bonus", 0.0);
    }
}
```

### behaviors/feedback.rune
```rune
use game::*;

pub fn on_hit(source, target, hit) {
    let intensity = if hit.is_crit { 1.0 } else { 0.5 };
    vfx("hit_flash", target.pos);
    sound("impact", target.pos);
    screen_shake(intensity);
    hit_stop(if hit.is_crit { 0.08 } else { 0.04 });
}
```

### abilities/melee_attack.rune
```rune
use game::*;

pub fn on_ability_start(source, ability) {
    animate(source, "attack");
    sound("swoosh", source.pos);

    let targets = targets_in_cone(source, source.attack_range, source.attack_arc);

    for target in targets {
        let hit = Hit {
            damage: source.attack_damage,
            knockback: source.knockback_force,
            is_crit: false,
        };

        fire_hook("on_pre_hit", source, target, hit);

        damage(target, hit.damage);
        knockback(target, hit.knockback);

        fire_hook("on_hit", source, target, hit);

        if hit.is_crit {
            vfx("crit_particles", target.pos);
            sound("crit_impact", target.pos);
        }
    }
}
```

### abilities/ground_pound.rune
```rune
use game::*;

pub fn on_ability_start(source, ability) {
    animate(source, "ground_pound");
    sound("ground_pound", source.pos);
    vfx("ground_pound_shockwave", source.pos);

    let targets = targets_in_radius(source.pos, 6.0);
    let base_damage = source.attack_damage * 4.0;

    for target in targets {
        let hit = Hit {
            damage: base_damage,
            knockback: source.knockback_force * 2.0,
            is_crit: false,
        };

        fire_hook("on_pre_hit", source, target, hit);

        damage(target, hit.damage);
        knockback(target, hit.knockback);

        fire_hook("on_hit", source, target, hit);

        if hit.is_crit {
            vfx("crit_particles", target.pos);
        }
    }

    screen_shake(1.5);
}
```

### enemies/zombie_ai.rune
```rune
use game::*;

pub fn on_tick(self, dt) {
    let player = nearest_player(self.pos);
    if player == () { return; }

    let dist = distance(self.pos, player.pos);

    if dist <= self.attack_range && self.cooldown_ready {
        // Attack
        set_behavior(self, "attack");
        let targets = targets_in_radius(self.pos, self.attack_range);
        for target in targets {
            let hit = Hit {
                damage: self.attack_damage,
                knockback: self.knockback_force,
                is_crit: false,
            };
            damage(target, hit.damage);
            knockback(target, hit.knockback);
        }
    } else if dist <= 25.0 {
        // Chase
        set_behavior(self, "chase");
        move_toward(self, player.pos, self.speed * dt);
    } else {
        set_behavior(self, "idle");
    }
}
```

## Script Loading

| Environment | Loading strategy |
|---|---|
| Client (native dev) | Hot-reload from `assets/scripts/` via Bevy asset watcher |
| Client (WASM web) | Bundled as Bevy assets, loaded at startup |
| Server (SpacetimeDB) | Embedded via `include_str!()` at compile time |

All scripts are compiled to Rune bytecode on load. The bytecode is cached in a `ScriptRegistry` resource/state. Hot-reload recompiles changed scripts and swaps the bytecode.

## Safety & Validation

- **Load-time signature validation**: When a script is compiled, we check exported functions have the expected arity for their hook type. Wrong number of params = load error.
- **Field access checking**: Rune structs enforce known fields. `source.typo_field` fails with "field not found" at the call site, not silently.
- **Command buffer isolation**: Scripts can't corrupt game state mid-execution. All mutations happen after the script returns.
- **Deterministic RNG**: `chance()` uses the existing deterministic seed system from `core/rng.rs` — same seed + same target = same result on client and server.

## Migration Plan (First Milestone)

| Current Rust | Becomes |
|---|---|
| `core/presets/crit.rs` + `crit.preset.ron` | `behaviors/crit.rune` |
| `core/presets/stacking.rs` + `stacking.preset.ron` | `behaviors/stacking.rune` |
| `core/presets/feedback.rs` | `behaviors/feedback.rune` |
| `core/combat.rs` → `resolve_combat()` | `abilities/melee_attack.rune` + `abilities/ground_pound.rune` |
| `server/enemy_ai.rs` → AI logic in `game_tick()` | `enemies/zombie_ai.rune` |
| `core/rules.rs` (Condition/Effect/Rule enums) | Removed — replaced by Rune scripts |

The Rust rules system (`rules.rs`, `Condition`, `Effect`, `Rule`, `Stats`, `ActionVars`) gets replaced entirely. Rune scripts are strictly more expressive than the enum-based rules.

## Open Questions

- **Rune in SpacetimeDB WASM**: Rune is pure Rust and should compile to WASM, but SpacetimeDB's sandbox may have restrictions. Validate early.
- **Performance with 10K enemies**: Each enemy running `on_tick` every 33ms is ~300K script invocations/sec. Benchmark Rune's VM throughput for this. May need to batch or throttle.
- **Script debugging**: Rune has basic diagnostics. May need a custom logger that captures script errors with context (which entity, which hook, what inputs).

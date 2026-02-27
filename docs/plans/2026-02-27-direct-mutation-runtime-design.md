# Direct Mutation Gameplay Runtime

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the command buffer scripting model with direct mutation, where Rune scripts mutate game state immediately through semantic primitives and can read the results of their own writes.

**Context:** The command buffer approach (scripts emit deferred commands, executors apply them) prevents scripts from reacting to their own effects. A melee attack can't check "did the target die?" because damage hasn't been applied yet. Direct mutation fixes this by making state changes immediate.

**Architecture:** Scripts call semantic primitives like `apply_damage(target, 50)` which mutate the Combatant struct in-place and log the intent. After the script returns, the engine reads the intent log to update real game state (ECS components on client, SpacetimeDB tables on server). Presentation effects (VFX, sound) go through a side-effect log on the client; the server provides no-op implementations.

**Tech Stack:** Rune 0.14.1, Bevy 0.18, SpacetimeDB 1.12.0

---

## Execution Model

```
Script calls apply_damage(target, 50)
  -> target.health mutated immediately (80 -> 30)
  -> Intent logged: DamageDealt { target_id: 7, amount: 50 }
  -> Script continues, can read target.health == 30

Script calls vfx("slash", target)
  -> Client: effect appended to log
  -> Server: no-op (function body is empty)

After script returns:
  -> Engine reads intent log -> updates real ECS/DB state
  -> Client reads effects log -> spawns particles, plays sounds
```

Scripts receive Combatant structs (cloned snapshots of entity state). Semantic primitives mutate these snapshots and record what happened. The engine diffs or reads the log to apply changes to the real world.

## API Surface

All functions are defined in `api.rs` and registered into the Rune module `gameplay` (scripts import via `use gameplay::*;`).

### State Primitives

Mutate the Combatant directly and log intent. Run on both client and server.

| Function | Signature | Description |
|---|---|---|
| `apply_damage` | `(target, amount) -> f32` | Reduce target health. Returns actual damage dealt (clamped to remaining health). |
| `heal` | `(target, amount) -> f32` | Increase target health up to max. Returns actual healing. |
| `apply_knockback` | `(target, force)` | Set knockback force on target. |
| `add_buff` | `(target, name, duration)` | Apply a named buff with duration. |
| `remove_buff` | `(target, name)` | Remove a named buff. |
| `set_stat` | `(entity, stat, value)` | Set a named stat on an entity. |
| `kill` | `(target)` | Set target health to 0. Logs a Kill intent. |

### Queries

Read-only functions. Run on both client and server.

| Function | Signature | Description |
|---|---|---|
| `chance` | `(probability) -> bool` | Returns true with given probability using the seeded RNG roll. |
| `distance` | `(a, b) -> f32` | 2D distance between two combatants. |
| `targets_in_cone` | `(source, range, arc) -> Vec` | All targets in a cone from source. |
| `targets_in_radius` | `(x, z, radius) -> Vec` | All targets in a radius. |
| `nearest_player` | `(x, z) -> Option<Combatant>` | Nearest player to a position. |
| `min` | `(a, b) -> f32` | Minimum of two values. |
| `max` | `(a, b) -> f32` | Maximum of two values. |

### Hooks

Behavior chaining system. Runs on both client and server.

| Function | Signature | Description |
|---|---|---|
| `fire_hook` | `(name, source, target, hit) -> Hit` | Chain a hook through all entity behaviors that implement it. |

### Presentation

Client: appended to effects log. Server: no-op (empty function body).

| Function | Signature | Description |
|---|---|---|
| `vfx` | `(name, target)` | Spawn a named visual effect on target. |
| `sound` | `(name, target)` | Play a named sound at target's position. |
| `screen_shake` | `(intensity)` | Shake the camera. |
| `hit_stop` | `(duration)` | Freeze-frame effect. |
| `animate` | `(entity, animation)` | Play a named animation on entity. |

## Types

### Combatant

Mutable snapshot of an entity's combat-relevant state. Fields are directly readable in scripts via `target.health`, `source.attack_damage`, etc.

Fields: `id`, `pos_x`, `pos_y`, `pos_z`, `dir_x`, `dir_z`, `health`, `max_health`, `attack_damage`, `crit_chance`, `crit_multiplier`, `knockback_force`, `attack_range`, `attack_arc`, `attack_speed`, `fury_stacks`, `attack_speed_bonus`, `cooldown_ready`, `speed`.

### Hit

Threaded through behavior hooks. Constructed in scripts.

Fields: `damage` (f32), `knockback` (f32), `is_crit` (bool).

## Intent & Effect Events

Defined in `api.rs` alongside the functions that produce them.

### Intent Events (state changes)

```rust
enum Intent {
    DamageDealt { target_id: u64, amount: f32 },
    Healed { target_id: u64, amount: f32 },
    KnockbackApplied { target_id: u64, force: f32 },
    BuffAdded { target_id: u64, name: String, duration: f32 },
    BuffRemoved { target_id: u64, name: String },
    StatSet { entity_id: u64, stat: String, value: f32 },
    Killed { target_id: u64 },
}
```

### Effect Events (presentation)

```rust
enum Effect {
    Vfx { name: String, target_id: u64 },
    Sound { name: String, target_id: u64 },
    ScreenShake { intensity: f32 },
    HitStop { duration: f32 },
    Animate { entity_id: u64, animation: String },
}
```

Both are stored in thread-local logs, drained after script execution.

## Client/Server Split

Same script, same functions, different runtime behavior:

- **Both sides:** State primitives mutate Combatant snapshots and log intents. Queries read thread-local state. After script returns, engine applies intents to real state (Bevy ECS or SpacetimeDB tables).
- **Client only:** Presentation functions append to effects log. After script returns, a Bevy system processes the effects log to spawn VFX, play sounds, etc.
- **Server only:** Presentation functions are no-ops (empty bodies, no log).

The server builds the Rune module without effect function implementations. Or more practically, the effect functions simply don't push to any log on the server.

## File Structure

```
core/src/runtime/       (was: core/src/scripting/)
  mod.rs                Engine: compile + execute Rune scripts
  api.rs                All functions, types, intents, effects (was: game_module.rs + commands.rs)
  registry.rs           ScriptRegistry: compiled script storage
  types.rs              Combatant, Hit

core/gameplay/          .rune script files
  abilities/            melee_attack.rune, ground_pound.rune
  behaviors/            crit.rune, stacking.rune, feedback.rune
  enemies/              zombie_ai.rune

client/src/scripting.rs Bevy plugin: registry resource, hot-reload, components
server/src/scripting.rs Server registry + execution wrappers
```

## Example: Melee Attack (Before/After)

### Before (Command Buffer)

```rune
use game::*;

pub fn on_ability_start(source, targets) {
    for target in targets {
        let hit = Hit { damage: source.attack_damage, knockback: source.knockback_force, is_crit: false };
        let hit = fire_hook("on_pre_hit", source, target, hit);

        damage(target, hit.damage);           // deferred! can't check result
        knockback(target, hit.knockback);
        // Can NOT do: if target.health <= 0 { ... }

        vfx("slash", target);
        fire_hook("on_hit", source, target, hit);
    }
}
```

### After (Direct Mutation)

```rune
use gameplay::*;

pub fn on_ability_start(source, targets) {
    for target in targets {
        let hit = Hit { damage: source.attack_damage, knockback: source.knockback_force, is_crit: false };
        let hit = fire_hook("on_pre_hit", source, target, hit);

        apply_damage(target, hit.damage);     // immediate! target.health updated now
        apply_knockback(target, hit.knockback);

        if target.health <= 0 {               // this works!
            vfx("death_explosion", target);
        }

        vfx("slash", target);
        fire_hook("on_hit", source, target, hit);
    }
}
```

## Migration

This replaces the command buffer system built in the previous iteration. Key changes:

1. `core/src/scripting/` renamed to `core/src/runtime/`
2. `game_module.rs` + `commands.rs` merged into `api.rs`
3. `Command` enum replaced by `Intent` + `Effect` enums
4. `CommandBuffer` replaced by thread-local intent/effect logs
5. All `damage()`, `heal()` etc. changed from buffer-push to direct mutation + intent log
6. Presentation functions become no-ops on server
7. Client/server executors updated to read intent log instead of command buffer
8. All `.rune` scripts updated: `use game::*` -> `use gameplay::*`
9. Scripts updated to use new function names where changed (e.g. `damage()` -> `apply_damage()`)

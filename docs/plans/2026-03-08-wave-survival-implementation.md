# Wave Survival Roguelite Loop — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform the flat-arena combat prototype into a playable roguelite survival loop with continuous horde, XP/leveling, stackable upgrades, and death/retry.

**Architecture:** Server-authoritative via SpacetimeDB. Horde spawning, XP, leveling, and upgrade state all live server-side. Client handles UI, visual feedback, and prediction. Upgrades execute as Rune behavior scripts on both client and server. New systems are Bevy plugins in the flat module architecture.

**Tech Stack:** Bevy 0.18, SpacetimeDB, Avian3d, Rune scripting, bevy_enhanced_input

---

## Phase 1: Core Game Loop

Get the die → retry loop working with automatic enemies before adding progression.

### Task 1: Player Death & Restart

Currently enemies can kill the player (Health reaches 0), but there's no game-over state. Add death detection, a simple death overlay, and restart.

**Files:**
- Create: `client/src/ui/death_screen.rs`
- Modify: `client/src/ui/mod.rs` — register death screen plugin
- Modify: `client/models/src/states.rs` — add `GameOver` to `Screen` enum
- Modify: `client/models/src/event_dispatch.rs` — add `RestartRun` event
- Modify: `server/src/lib.rs` — add `restart_run` reducer

**Step 1: Add GameOver screen state**

In `client/models/src/states.rs`, add `GameOver` variant to the `Screen` enum:

```rust
pub enum Screen {
    Splash,
    Loading,
    Tutorial,
    Settings,
    Title,
    Connecting,
    Gameplay,
    GameOver,  // NEW
}
```

**Step 2: Add RestartRun event**

In `client/models/src/event_dispatch.rs`:

```rust
#[derive(Event)]
pub struct RestartRun;
```

**Step 3: Create death screen UI**

Create `client/src/ui/death_screen.rs`:

- On `OnEnter(Screen::GameOver)`: spawn a centered overlay with "You Died" text and a "Try Again" button
- On button press: fire `RestartRun` event
- On `OnExit(Screen::GameOver)`: despawn the overlay

Keep it minimal — a dark semi-transparent background, white text, one button. Use existing UI patterns from `client/src/screens/title.rs` for reference.

**Step 4: Wire death → GameOver transition**

Currently `on_death()` in `client/src/combat/damage.rs` (line 96) only despawns non-server entities. For the local player:

- Detect player death (entity has `PlayerCombatant` marker + `Health.is_dead()`)
- Transition to `Screen::GameOver` via `GoTo(Screen::GameOver)` event

**Step 5: Implement restart flow**

On `RestartRun` event:
- Client sends `restart_run` reducer to server
- Server reducer resets player health to max, resets position to origin, clears all enemies in world
- Client transitions back to `Screen::Gameplay`
- Reuse existing `clear_enemies` reducer (server/src/enemy_ai.rs line 65) for enemy cleanup

**Step 6: Register plugin**

Add `death_screen` module to `client/src/ui/mod.rs` plugin registration.

**Step 7: Test manually**

Run: `just`
- Let enemies kill you → death screen should appear
- Click "Try Again" → should restart with full health, no enemies
- Verify multiplayer still works: death/restart is per-player

**Step 8: Commit**

```
feat: add death screen and restart flow
```

---

### Task 2: Horde Spawner (Server-Side)

Replace E-key manual spawning with continuous automatic enemy spawning that escalates over time.

**Files:**
- Create: `server/src/horde.rs` — spawner logic
- Modify: `server/src/lib.rs` — register horde module, add start/stop reducers
- Modify: `server/src/schema.rs` — add `HordeState` table
- Modify: `server/src/enemy_ai.rs` — integrate horde spawning into `game_tick()`
- Modify: `core/src/combat.rs` — add enemy variant constants

**Step 1: Define enemy variant constants**

In `core/src/combat.rs`, add variant types and their stats:

```rust
pub const ENEMY_TYPE_BASIC: u8 = 0;
pub const ENEMY_TYPE_FAST: u8 = 1;
pub const ENEMY_TYPE_BRUTE: u8 = 2;

pub fn enemy_defaults(enemy_type: u8) -> (f32, f32, f32, f32, f32) {
    // Returns (health, damage, speed, attack_range, attack_speed)
    match enemy_type {
        ENEMY_TYPE_FAST => (200.0, 10.0, 4.0, 2.0, 1.5),
        ENEMY_TYPE_BRUTE => (1200.0, 25.0, 1.2, 2.5, 0.5),
        _ => (ENEMY_HEALTH, ENEMY_ATTACK_DAMAGE, ENEMY_WALK_SPEED, ENEMY_ATTACK_RANGE, 1.0),
    }
}
```

**Step 2: Add HordeState table**

In `server/src/schema.rs`:

```rust
#[spacetimedb::table(accessor = horde_state, public)]
pub struct HordeState {
    #[primary_key]
    pub world_id: u32,
    pub active: bool,
    pub elapsed_secs: f32,      // Total time since horde started
    pub spawn_accumulator: f32, // Fractional spawns accumulated
}
```

**Step 3: Create horde spawner module**

Create `server/src/horde.rs`:

- `start_horde(world_id)` — insert `HordeState` row, set active=true
- `stop_horde(world_id)` — set active=false
- `tick_horde(world_id, dt)` — called from `game_tick()`:
  - Calculate spawn rate from elapsed time: `base_rate + elapsed * ramp`
    - Base: 1.0/sec, ramp: +0.05/sec per second (at 60s = 4/sec, at 120s = 7/sec)
  - Pick enemy type based on elapsed time thresholds (from design doc)
  - Spawn around a random player in the world, in a ring outside camera view (30-50m radius)
  - Use existing scatter hash approach from `spawn_enemies()`

**Step 4: Integrate into game_tick**

In `server/src/enemy_ai.rs` `game_tick()`, after AI processing, call `horde::tick_horde()` if horde is active. The dt comes from `TICK_INTERVAL_MICROS`.

**Step 5: Auto-start horde on gameplay begin**

When a player connects and enters a world (in `connect` or existing identity_connected handler), if no horde is active for that world_id, start one. Alternatively, start horde when `restart_run` is called.

**Step 6: Remove E-key spawning (optional — can keep for debug)**

Keep E-key behind `#[cfg(feature = "dev")]` or just leave it. Don't break the dev workflow.

**Step 7: Test manually**

Run: `just`
- Enemies should start appearing automatically after connecting
- Density should visibly increase over 2 minutes
- After restart, horde should reset

**Step 8: Commit**

```
feat: add continuous horde spawner with escalating difficulty
```

---

### Task 3: Enemy Variants (Client-Side Visuals)

The server now spawns different `enemy_type` values. The client needs to visually distinguish them.

**Files:**
- Modify: `client/src/combat/enemy.rs` — apply scale/tint based on `enemy_type`
- Modify: `client/models/src/combat.rs` — add `EnemyType` enum with display properties

**Step 1: Add EnemyType enum**

In `client/models/src/combat.rs`:

```rust
#[derive(Component, Default, Clone, Copy, PartialEq, Eq, Reflect, Debug)]
#[repr(u8)]
pub enum EnemyType {
    #[default]
    Basic = 0,
    Fast = 1,
    Brute = 2,
}

impl EnemyType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Fast,
            2 => Self::Brute,
            _ => Self::Basic,
        }
    }

    pub fn scale(&self) -> f32 {
        match self {
            Self::Basic => 1.0,
            Self::Fast => 0.8,
            Self::Brute => 1.4,
        }
    }

    pub fn walk_speed(&self) -> f32 {
        match self {
            Self::Basic => 2.0,
            Self::Fast => 4.0,
            Self::Brute => 1.2,
        }
    }
}
```

**Step 2: Apply visual differentiation on spawn**

In `client/src/combat/enemy.rs`, in the enemy spawn observer (where VAT mesh is attached), read the `enemy_type` field from the synced data and:
- Set transform scale based on `EnemyType::scale()`
- Optionally apply a color tint via material override (Fast = greenish, Brute = reddish)

**Step 3: Use walk speed from type in chase animation**

Ensure the enemy movement interpolation uses the type-specific walk speed.

**Step 4: Test manually**

Run: `just`
- After 60s, smaller fast zombies should appear
- After 120s, larger brute zombies should appear
- Visual size difference should be immediately obvious

**Step 5: Commit**

```
feat: add fast and brute zombie variants with visual differentiation
```

---

### Task 4: XP Orbs

Enemies drop XP orbs on death. Player walks over them to collect. This is the core progression input.

**Files:**
- Create: `client/src/combat/xp.rs` — orb spawning, pickup, XP tracking
- Modify: `client/src/combat/mod.rs` — register xp module
- Modify: `client/models/src/combat.rs` — add `XpOrb`, `PlayerXp` types
- Modify: `server/src/schema.rs` — add `PlayerXp` fields to Player table or new table
- Modify: `server/src/combat.rs` — emit XP data on enemy death

**Step 1: Decide client-only vs server-authoritative XP**

For now, implement XP as **client-side only**. Rationale:
- XP is a session-only value (resets on death)
- No competitive integrity concern for singleplayer roguelite
- Avoids schema changes and reducer round-trips for every orb pickup
- Server authority can be added later if needed for multiplayer anti-cheat

**Step 2: Add XP types**

In `client/models/src/combat.rs`:

```rust
#[derive(Component)]
pub struct XpOrb {
    pub value: f32,
    pub magnet_speed: f32,
}

#[derive(Resource, Default)]
pub struct PlayerXp {
    pub current: f32,
    pub level: u32,
    pub to_next_level: f32,
    pub total_collected: f32,
}

impl PlayerXp {
    pub fn xp_for_level(level: u32) -> f32 {
        100.0 * 1.15_f32.powi(level as i32)
    }

    pub fn add_xp(&mut self, amount: f32) -> bool {
        self.current += amount;
        self.total_collected += amount;
        if self.current >= self.to_next_level {
            self.current -= self.to_next_level;
            self.level += 1;
            self.to_next_level = Self::xp_for_level(self.level);
            return true; // leveled up
        }
        false
    }
}
```

**Step 3: Create XP module**

Create `client/src/combat/xp.rs`:

**Orb spawning system:**
- Listen for enemy death events (the existing `Died` event or `on_death` observer)
- At death position, spawn a small glowing sphere entity with:
  - `XpOrb { value, magnet_speed: 8.0 }`
  - `Transform` at enemy death position
  - A simple mesh (sphere) + emissive material (bright green/yellow)
  - No physics body — just transform-based movement
- XP value based on enemy type: Basic=10, Fast=8, Brute=25

**Orb pickup system (runs every frame):**
- Query all `XpOrb` entities and the player position
- For orbs within pickup radius (2.0m): despawn orb, add XP to `PlayerXp` resource
- For orbs within magnet radius (5.0m): lerp orb position toward player (magnet effect)

**Level-up detection:**
- When `PlayerXp::add_xp()` returns true, fire a `LevelUp` event

**Reset on restart:**
- When `RestartRun` fires, reset `PlayerXp` to default

**Step 4: Register module**

Add `xp` module to `client/src/combat/mod.rs`.

**Step 5: Test manually**

Run: `just`
- Kill enemies → green orbs should appear at death location
- Walk near orbs → they drift toward you then get collected
- Watch console/debug for XP accumulation and level-up prints

**Step 6: Commit**

```
feat: add XP orb drops and leveling system
```

---

### Task 5: HUD — XP Bar, Level, Kill Counter

Show the player their progression during a run.

**Files:**
- Create: `client/src/ui/run_hud.rs` — run-specific HUD elements
- Modify: `client/src/ui/mod.rs` — register run HUD plugin
- Modify: `client/models/src/combat.rs` — add `KillCounter` resource

**Step 1: Add KillCounter resource**

In `client/models/src/combat.rs`:

```rust
#[derive(Resource, Default)]
pub struct KillCounter(pub u32);
```

Increment in the existing death observer. Reset on `RestartRun`.

**Step 2: Create run HUD**

Create `client/src/ui/run_hud.rs`. Spawn on `OnEnter(Screen::Gameplay)`, despawn on `OnExit(Screen::Gameplay)`:

**XP Bar (bottom center):**
- Container: 300px wide, 12px tall, dark background
- Fill bar: colored (gold/yellow), width = `(player_xp.current / player_xp.to_next_level) * 100%`
- Updates every frame from `PlayerXp` resource

**Level indicator (left of XP bar):**
- Text node: "Lv. {level}"
- Updates when level changes

**Kill counter (top right):**
- Text node: "Kills: {count}"
- Updates from `KillCounter` resource

**Step 3: Register plugin**

Add to `client/src/ui/mod.rs`.

**Step 4: Test manually**

Run: `just`
- XP bar visible at bottom, fills as you collect orbs
- Level number increases on level-up
- Kill counter increments on each kill

**Step 5: Commit**

```
feat: add run HUD with XP bar, level, and kill counter
```

---

### Task 6: Upgrade Selection UI

The core roguelite mechanic — pause the game on level-up, show 3 upgrade cards, let the player pick one.

**Files:**
- Create: `client/src/ui/upgrade_select.rs` — upgrade selection overlay
- Create: `client/src/combat/upgrades.rs` — upgrade definitions, inventory, application
- Modify: `client/src/combat/mod.rs` — register upgrades module
- Modify: `client/src/ui/mod.rs` — register upgrade select plugin
- Modify: `client/models/src/combat.rs` — add upgrade-related types

**Step 1: Define upgrade data types**

In `client/models/src/combat.rs`:

```rust
#[derive(Clone, Debug)]
pub struct UpgradeDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,  // Base description
    pub breakpoints: &'static [(u32, &'static str)],  // (stack_threshold, description)
}

#[derive(Resource, Default)]
pub struct UpgradeInventory {
    pub held: HashMap<&'static str, u32>,  // upgrade_id → stack count
}

#[derive(Event)]
pub struct LevelUp;

#[derive(Event)]
pub struct UpgradeSelected(pub &'static str);
```

**Step 2: Define starter upgrades**

In `client/src/combat/upgrades.rs`, define the initial upgrade pool. Start with **simple stat upgrades** — we'll convert to Rune-based status effect upgrades in Phase 2:

```rust
pub static UPGRADE_POOL: &[UpgradeDef] = &[
    UpgradeDef {
        id: "fury_amp",
        name: "Fury Amplifier",
        description: "+20% fury stack bonus (12% → 14.4%)",
        breakpoints: &[(3, "Fury decays 50% slower"), (5, "Max stacks +6"), (10, "Fury never decays")],
    },
    UpgradeDef {
        id: "vampiric",
        name: "Vampiric Strikes",
        description: "Heal 5% of damage dealt",
        breakpoints: &[(3, "Overheal grants shield"), (5, "Leech applies to all damage"), (10, "+25% damage above 90% HP")],
    },
    UpgradeDef {
        id: "heavyweight",
        name: "Heavyweight",
        description: "+40% knockback, +15% damage",
        breakpoints: &[(3, "Knocked enemies damage others"), (5, "Ground pound radius +50%"), (10, "Knockback scales with combo")],
    },
    UpgradeDef {
        id: "magnetism",
        name: "Magnetism",
        description: "XP pickup radius doubled",
        breakpoints: &[(3, "Orbs worth +25%"), (5, "Orbs heal 1 HP"), (10, "Orbs pull from entire screen")],
    },
    UpgradeDef {
        id: "berserker",
        name: "Berserker",
        description: "Below 30% HP: +50% attack speed",
        breakpoints: &[(3, "Threshold → 50% HP"), (5, "+25% damage in berserk"), (10, "Cannot die for 3s after entering berserk")],
    },
    UpgradeDef {
        id: "crit_surge",
        name: "Critical Surge",
        description: "+10% crit chance",
        breakpoints: &[(3, "Crits chain to 1 enemy"), (5, "Crit multiplier +0.5"), (10, "Crits reset attack cooldown")],
    },
    UpgradeDef {
        id: "ground_tremor",
        name: "Ground Tremor",
        description: "Ground pound radius +50%",
        breakpoints: &[(3, "Ground pound slows enemies"), (5, "Ground pound pulls enemies in"), (10, "Airborne → auto ground pound on landing")],
    },
    UpgradeDef {
        id: "echo_strike",
        name: "Echo Strike",
        description: "20% chance to repeat last attack",
        breakpoints: &[(3, "Echoes can trigger echoes"), (5, "Echo chance +15%"), (10, "Every 5th hit guaranteed echo")],
    },
];
```

**Step 3: Implement upgrade application**

In `client/src/combat/upgrades.rs`:

- `apply_upgrade(id, inventory, stats)` — increment stack count, apply stat modifications
- For Phase 1, upgrades modify `Stats` component values directly
- Phase 2 will replace this with Rune behavior script attachment

Concrete Phase 1 implementations (stat-only, no status effects):
- `fury_amp`: modify fury stack bonus multiplier on Stats
- `vampiric`: set a leech % on Stats, heal in damage observer
- `heavyweight`: modify knockback_force and attack_damage on Stats
- `magnetism`: modify pickup radius on PlayerXp or a new resource
- `berserker`: check health threshold in attack speed calculation
- `crit_surge`: modify crit_chance on Stats
- `ground_tremor`: modify ground pound radius
- `echo_strike`: proc chance checked in attack system

**Step 4: Implement upgrade selection offering**

- On `LevelUp` event: pick 3 random upgrades from `UPGRADE_POOL`
  - Allow duplicates of already-held upgrades (that's stacking)
  - Weight toward upgrades player doesn't have yet? (optional, can be pure random)
- Store offered upgrades in a `PendingUpgradeChoice` resource

**Step 5: Create upgrade selection UI**

Create `client/src/ui/upgrade_select.rs`:

- On `LevelUp` event:
  - Set `Time<Virtual>` paused (same mechanism as ESC pause)
  - Spawn overlay: semi-transparent dark background, 3 upgrade cards in a row
  - Each card shows: upgrade name, description, current stacks (if any), next breakpoint preview
  - Highlight card on hover, select on click

- On card click:
  - Fire `UpgradeSelected(id)` event
  - Despawn overlay
  - Resume `Time<Virtual>`

- Card layout:
  - 3 cards centered horizontally, ~200px wide each, ~280px tall
  - Name at top (bold, larger font)
  - Description in middle
  - Stack count badge (if owned): "×3" in corner
  - Next breakpoint preview at bottom in muted text: "At ×5: Leech applies to all damage"

**Step 6: Wire LevelUp → UI → apply**

- `client/src/combat/xp.rs`: on `add_xp()` returning true, fire `LevelUp` event
- `client/src/ui/upgrade_select.rs`: on `LevelUp`, pause + show cards
- `client/src/combat/upgrades.rs`: on `UpgradeSelected`, apply upgrade to player

**Step 7: Reset on restart**

On `RestartRun`: reset `UpgradeInventory`, reset all stat modifications to defaults.

**Step 8: Test manually**

Run: `just`
- Kill enemies, collect XP → level up → game pauses → 3 cards appear
- Click a card → game resumes, upgrade is applied
- Pick same upgrade multiple times → stack count shows on card
- Die → restart → upgrades reset

**Step 9: Commit**

```
feat: add upgrade selection UI with 8 starter upgrades
```

---

### Task 7: Death Screen Polish

Enhance the minimal death screen from Task 1 with run statistics.

**Files:**
- Modify: `client/src/ui/death_screen.rs` — add stats display

**Step 1: Track run time**

Add a `RunTimer` resource (f32, increments with `Time<Virtual>` delta). Reset on restart.

**Step 2: Display run stats on death screen**

Show on the death overlay:
- Time survived (formatted as M:SS)
- Enemies killed (from `KillCounter`)
- Level reached (from `PlayerXp`)
- Upgrades taken (list from `UpgradeInventory` with stack counts)

**Step 3: Test manually**

Run: `just`
- Play a full run → die → death screen shows all stats
- Stats should accurately reflect the run

**Step 4: Commit**

```
feat: add run statistics to death screen
```

---

## Phase 2: Status Effects & Full Upgrade Trees

Layer the composable tag system on top of the working game loop.

### Task 8: Status Effect System

Implement the 4 status effect primitives that upgrades will apply and reference.

**Files:**
- Create: `client/src/combat/status_effects.rs` — status effect logic
- Modify: `client/src/combat/mod.rs` — register module
- Modify: `client/models/src/combat.rs` — add status effect types
- Modify: `core/src/runtime/api.rs` — add status effect intents
- Modify: `server/src/combat.rs` — process status effect intents

**Step 1: Define status effect types**

In `client/models/src/combat.rs`:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
pub enum StatusEffectKind {
    Burn,
    Slow,
    Mark,
    Stagger,
}

#[derive(Clone, Debug, Reflect)]
pub struct StatusEffect {
    pub kind: StatusEffectKind,
    pub intensity: f32,     // Burn: dps, Slow: speed reduction %, Mark: damage amp %, Stagger: unused
    pub remaining: f32,     // Seconds remaining
    pub max_duration: f32,  // For refresh-on-reapply
}

#[derive(Component, Default, Reflect)]
pub struct StatusEffects {
    pub effects: HashMap<StatusEffectKind, StatusEffect>,
}
```

**Step 2: Status effect tick system**

In `client/src/combat/status_effects.rs`:

- **Tick system** (every frame): decrement `remaining` by dt, remove expired effects
- **Burn tick**: apply DPS damage to entity's `Health` component
- **Slow effect**: multiply enemy movement speed by `(1.0 - slow_intensity)`
- **Mark effect**: store damage multiplier, checked during damage calculation
- **Stagger effect**: set enemy behavior to Idle (stunned), prevent Chase/Attack transitions

**Step 3: Add apply/refresh API**

```rust
pub fn apply_status(effects: &mut StatusEffects, kind: StatusEffectKind, intensity: f32, duration: f32) {
    if let Some(existing) = effects.effects.get_mut(&kind) {
        existing.intensity = existing.intensity.max(intensity); // Take stronger
        existing.remaining = duration; // Refresh duration
    } else {
        effects.effects.insert(kind, StatusEffect { kind, intensity, remaining: duration, max_duration: duration });
    }
}
```

**Step 4: Visual feedback**

Minimal visual indicators:
- Burn: orange-tinted material or particle
- Slow: blue-tinted material
- Mark: pulsing white outline or bright highlight
- Stagger: brief white flash

Keep these simple — a color tint on the enemy mesh is sufficient for Phase 2.

**Step 5: Extend Rune API**

In `core/src/runtime/api.rs`, add new intents:

```rust
pub enum Intent {
    // ... existing intents ...
    StatusApplied { target_id: u64, kind: u8, intensity: f32, duration: f32 },
    StatusRemoved { target_id: u64, kind: u8 },
}
```

Add Rune-callable functions:
- `apply_burn(target, intensity, duration)`
- `apply_slow(target, intensity, duration)`
- `apply_mark(target, intensity, duration)`
- `apply_stagger(target, duration)`
- `has_status(target, kind) -> bool`
- `get_status_intensity(target, kind) -> f32`

**Step 6: Add StatusEffects component to enemies**

In the enemy spawn observer (`client/src/combat/enemy.rs`), add `StatusEffects::default()` to spawned enemies.

**Step 7: Integrate Mark with damage calculation**

In the damage resolution path, check if target has Mark status and multiply damage by `(1.0 + mark_intensity)`.

**Step 8: Test manually**

Test by temporarily adding status application on hit:
- Burn: enemy health decreases over time after being hit
- Slow: enemy moves noticeably slower
- Mark: subsequent hits deal more damage
- Stagger: enemy freezes briefly

**Step 9: Commit**

```
feat: add status effect system (Burn, Slow, Mark, Stagger)
```

---

### Task 9: Attack Modifiers

Implement Chain, Pierce, Area, and Leech as composable attack properties.

**Files:**
- Create: `client/src/combat/attack_modifiers.rs`
- Modify: `client/models/src/combat.rs` — add modifier types
- Modify: `core/src/runtime/api.rs` — add modifier intents
- Modify: `core/src/combat.rs` — add modifier resolution helpers

**Step 1: Define attack modifier types**

In `client/models/src/combat.rs`:

```rust
#[derive(Clone, Debug, Default, Reflect)]
pub struct AttackModifiers {
    pub chain_targets: u32,      // 0 = no chain
    pub chain_chance: f32,       // Proc chance
    pub pierce: bool,
    pub pierce_damage_mult: f32, // Damage multiplier for pierced targets
    pub area_radius: f32,        // 0 = no area
    pub area_damage_mult: f32,   // Damage multiplier for area targets
    pub leech_percent: f32,      // 0 = no leech
}
```

**Step 2: Implement modifier resolution**

In `client/src/combat/attack_modifiers.rs`:

- **Chain**: after a hit, find N nearest enemies within chain range, apply reduced damage. Each chained hit can carry status effects if upgraded.
- **Pierce**: after hitting primary target, raycast forward and damage all enemies in the line.
- **Area**: after hitting primary target, damage all enemies within radius of impact point.
- **Leech**: after dealing damage, heal player by `leech_percent * damage_dealt`.

These run as post-processing after the core hit resolution. The Rune `on_hit` hook fires first (for status effects), then modifiers expand the damage to additional targets.

**Step 3: Add Rune API functions**

```rust
// Available in Rune scripts
chain_attack(source, target, count, range)
pierce_attack(source, target, direction, max_distance)
area_damage(position, radius, damage)
heal(source, amount)
```

**Step 4: Store modifiers on player**

Add `AttackModifiers` as a component on the player entity. Upgrades modify this directly. The attack system reads it after each hit.

**Step 5: Test each modifier**

Temporarily grant each modifier and verify:
- Chain: lightning-arc visual from hit target to nearby enemies
- Pierce: hit passes through a line of enemies
- Area: visible splash damage around hit point
- Leech: player health recovers on hit

**Step 6: Commit**

```
feat: add attack modifiers (Chain, Pierce, Area, Leech)
```

---

### Task 10: Convert Upgrades to Full Rune-Based System

Replace the Phase 1 stat-only upgrades with Rune scripts that use status effects and attack modifiers. Implement breakpoint mutations.

**Files:**
- Create: `core/runes/upgrades/inferno.rune`
- Create: `core/runes/upgrades/permafrost.rune`
- Create: `core/runes/upgrades/hunters_eye.rune`
- Create: `core/runes/upgrades/concussion.rune`
- Create: `core/runes/upgrades/arc_conductor.rune`
- Create: `core/runes/upgrades/impaler.rune`
- Create: `core/runes/upgrades/vampiric.rune`
- Create: `core/runes/upgrades/aftershock.rune`
- Modify: `client/src/combat/upgrades.rs` — wire Rune scripts to upgrade application
- Modify: `server/src/scripting.rs` — register upgrade scripts
- Modify: `core/src/runtime/registry.rs` — load upgrade scripts

**Step 1: Design the upgrade script interface**

Each upgrade Rune script exports hooks with a `stacks` parameter:

```rust
// Standard upgrade script interface
pub fn on_hit(source, target, hit, stacks) -> hit { ... }
pub fn on_crit(source, target, hit, stacks) -> hit { ... }
pub fn on_kill(source, target, stacks) { ... }
pub fn on_damaged(source, attacker, damage, stacks) -> damage { ... }
pub fn on_dodge(source, stacks) { ... }
pub fn on_ground_pound(source, targets, stacks) { ... }
```

Not every script needs every hook — they only export the ones they use.

**Step 2: Implement Inferno (Burn tree) as the template**

```rust
// core/runes/upgrades/inferno.rune
pub fn on_hit(source, target, hit, stacks) {
    let chance_pct = 30.0 + (stacks - 1) * 5.0;
    let dps = 5.0 + (stacks - 1) * 5.0;
    let tick_mult = if stacks >= 3 { 2.0 } else { 1.0 };

    if chance(chance_pct / 100.0) {
        apply_burn(target, dps * tick_mult, 3.0);
    }

    if stacks >= 5 {
        // Burn spread handled in tick system
        set_stat(source, "burn_spreads", 1.0);
    }

    hit
}

pub fn on_kill(source, target, stacks) {
    if stacks >= 10 && has_status(target, "burn") {
        let burn_dps = get_status_intensity(target, "burn");
        area_damage(target, 4.0, burn_dps * 3.0);
    }
}
```

**Step 3: Implement remaining 7 upgrade scripts**

Follow the same pattern — each script handles its own hooks and checks `stacks` for breakpoint mutations. Reference the design doc for exact breakpoint behaviors.

**Step 4: Modify upgrade application to attach Rune behaviors**

When an upgrade is selected:
- If first stack: add the upgrade's script to the player's `EntityBehaviors` list
- Update stack count in `UpgradeInventory`
- Pass `stacks` to all script hook calls

**Step 5: Modify the behavior execution pipeline**

In `core/src/runtime/mod.rs`, extend `call_ability_with_behaviors()` to pass upgrade stack counts when calling hooks. Each registered behavior gets its stack count from the player's `UpgradeInventory`.

**Step 6: Test each upgrade through 10 stacks**

For each upgrade, test:
- Stack 1: base effect works
- Stack 3: first breakpoint mutation activates
- Stack 5: second breakpoint mutation activates
- Stack 10: final breakpoint mutation activates

**Step 7: Commit**

```
feat: implement 8 Rune-based upgrades with breakpoint mutations
```

---

### Task 11: Active Upgrades HUD Display

Show currently held upgrades with stack counts on the HUD.

**Files:**
- Modify: `client/src/ui/run_hud.rs` — add upgrade icon strip

**Step 1: Add upgrade display**

Left edge of screen, vertical strip of small icons/badges:
- Each held upgrade shows as a small card: abbreviated name + stack count
- Breakpoint thresholds indicated (e.g., stack count glows at 3/5/10)
- Ordered by acquisition time

**Step 2: Update on upgrade selection**

Rebuild the upgrade strip whenever `UpgradeSelected` fires.

**Step 3: Commit**

```
feat: add active upgrades display to HUD
```

---

## Implementation Order Summary

| # | Task | Dependencies | Milestone |
|---|------|-------------|-----------|
| 1 | Death & Restart | None | Can die and retry |
| 2 | Horde Spawner | None | Enemies come automatically |
| 3 | Enemy Variants | Task 2 | Visual variety |
| 4 | XP Orbs & Leveling | None | Progression input |
| 5 | Run HUD | Task 4 | Player sees progress |
| 6 | Upgrade Selection UI | Task 4 | **Core loop complete** |
| 7 | Death Screen Polish | Task 1, 4, 6 | Full retry experience |
| 8 | Status Effects | None | Upgrade foundation |
| 9 | Attack Modifiers | None | Upgrade foundation |
| 10 | Rune-Based Upgrades | Task 6, 8, 9 | **Full upgrade system** |
| 11 | Active Upgrades HUD | Task 6 | UI polish |

**Playable milestone after Task 6** — core roguelite loop works with simple stat upgrades.

**Full system after Task 10** — composable status effects, attack modifiers, and breakpoint mutations.

Tasks 1-2 and 4-5 can be parallelized. Tasks 8-9 can be parallelized.

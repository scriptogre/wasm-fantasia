# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

**Native Development**
- `just` - Run native dev build with dev_native features
- `just build` - Build native release
- `just check` - Pre-commit checks: clippy, fmt, machete, web compilation check

**Multiplayer**
- `just mp` - Start SpacetimeDB server, deploy module, launch two game clients

**Testing**
- `cargo test` - Run all tests (currently minimal)

## Project Architecture

Bevy 0.17 3D action RPG targeting native and WebAssembly. Flat module architecture within each crate.

### Workspace

| Crate | Purpose |
|-------|---------|
| `client/` | Bevy game client — all gameplay, rendering, UI, audio |
| `shared/` | Pure functions shared between client and server (combat resolution, rules, RNG). No Bevy types. |
| `server/` | SpacetimeDB module — authoritative game state, reducers |
| `crates/spacetimedb-sdk/` | Local SpacetimeDB SDK fork |

### Major Systems (all under `client/src/`)

- **combat** — Attacks, damage, targeting, hit detection, hit feedback (screen shake, hit stop, damage numbers, VFX), enemy spawning, combat sounds
- **rules** — Data-driven behavior system via triggers and composable building blocks (stats, conditions, effects)
- **rule_presets** — Reusable rule compositions (crit, stacking buff)
- **player** — Character control (Tnua + Avian3d physics), animation state machine, footstep sounds
- **networking** — SpacetimeDB connection, player sync, combat sync, auto-generated bindings
- **camera** — Third-person orbit camera (Metin2-style, elevated pitch for combat visibility)
- **audio** — bevy_seedling (Firewheel) music and sound (native only, WASM has dependency conflicts)
- **scene** — Environment loading via bevy_skein (Blender workflow), skybox with day/night cycle
- **screens** — Screen state management (splash, loading, title, settings, credits, gameplay), modal system
- **ui** — Reusable UI components: modals, settings panels, keybinding editors, interaction observers
- **models** — Shared data layer: input actions, game state, settings, screen states
- **asset_loading** — Centralized asset loading with RON config, progress tracking
- **postfx** — ReShade-style post-processing (color grading, toggle with F2)
- **game** — Dev tools (egui inspector), music spawning

### Input System (bevy_enhanced_input)

Two input contexts: `PlayerCtx` (gameplay) and `ModalCtx` (menus). Observer pattern: `On<Start<Action>>`, `On<Complete<Action>>`, `On<Fire<Action>>`.

### Animation Pipeline

`player.source.glb` contains the full Quaternius animation library. `build.rs` parses `Animation::clip_name()` and generates an optimized `player.glb` with only registered clips. To add an animation: add the enum variant and its clip_name/from_clip_name mappings — the build pipeline handles the rest.

### Physics (avian3d)

Player is Dynamic RigidBody with capsule Collider. TnuaAvian3dSensorShape for ground detection. Friction::ZERO with Multiply combine rule. Scene colliders loaded from GLTF via bevy_skein.

### Feature Flags

- `web` - Enables wasm32 target
- `audio` - Enables bevy_seedling audio (native only)
- `third_person` - Orbit camera (default, required)
- `dev_native` - Dev tools, inspector, asset hot-reloading (native)
- `multiplayer` - Enables SpacetimeDB networking

### Key Patterns

1. **System Sets**: `PostPhysicsAppSystems` defines ordering: UserInput -> TickTimers -> ChangeUi -> PlaySounds -> PlayAnimations -> Update
2. **Markers**: `PlayerCtx`, `ModalCtx`, `SceneCamera`, `DespawnOnExit` for query filtering
3. **Observers**: Heavily used for input handling and entity lifecycle
4. **Resources**: Config (RON), Settings (serializable), GameState (app-wide state)
5. **Camera Sync**: CameraSyncSet runs before TransformSystems::Propagate in PostUpdate

### Common Pitfalls

- Audio on web requires SharedArrayBuffer headers (COOP/COEP) or audio will stutter
- Hotpatching requires BEVY_ASSET_ROOT="." environment variable
- bevy_skein stores enum discriminants, not strings — library updates may break saved scenes
- Player rotation is locked on X/Z but free on Y (LockedAxes::ROTATION_LOCKED.unlock_rotation_y())
- TNUA movement is in FixedUpdate schedule, not Update

## Architecture Conventions

Reference: `docs/architecture/PATTERNS.md` (sources: Overwatch, Quake 3, Source Engine, Flecs, Bevy community)

### Module Dependency Direction

Strict import hierarchy. Violations are bugs.

```
networking → combat, player, models    (networking may import domain types)
combat → shared, models                (combat never imports networking)
player → shared, models, combat        (player may read combat components)
ui → models, combat, rules             (ui reads state, never mutates)
models → (nothing game-specific)       (pure data definitions)
shared → (no Bevy types at all)        (pure functions only)
```

**The rule**: Domain modules (combat, player) never import networking. Networking is a transport layer — it adapts between SpacetimeDB and domain events. If combat needs to "tell the server something," it fires a domain event that networking observes.

### Event Flow

Three-phase flow. Every state change follows this path:

```
Request (imperative)  →  Resolve (shared)  →  Outcome (past tense)  →  Feedback (cosmetic)
AttackIntent              resolve_combat()     DamageDealt              HitLanded
SpawnEnemyRequest         validate/position     EnemySpawned             (VFX, sound)
```

**Naming convention**:
- **Requests**: noun/imperative — `AttackIntent`, `SpawnEnemyRequest`. Hasn't happened yet.
- **Outcomes**: past tense — `DamageDealt`, `Died`, `EnemySpawned`. It happened, state changed.
- **Feedback**: past tense — `HitLanded`, `CritHit`. Cosmetic-only, never mutates game state.

Requests flow inward (input/networking → logic). Outcomes flow outward (logic → presentation/networking). Feedback is terminal — nothing observes feedback events to produce more state changes.

### Entity Spawning Ownership

The module that owns a domain concept owns its entity archetype. Period.

- `combat/enemy.rs` owns enemy entity bundles (Health, Stats, Combatant, Collider, mesh, etc.)
- `player/` owns player entity bundles
- Networking **never** constructs domain bundles. It fires a spawn request event carrying server data. The owning module observes it and builds the entity.

Why: When you add a new component to enemies (e.g., AI, pathfinding), you change one file in combat/, not also networking/combat.rs.

### Server Authority Marker

Use a `ServerAuthoritative` marker component on entities whose state is owned by the server. Domain modules check this marker instead of checking for networking-specific components with `cfg(feature)`.

```rust
// Good: combat/damage.rs
if server_auth.get(entity).is_ok() { /* skip local health mutation */ }

// Bad: combat/damage.rs
#[cfg(feature = "multiplayer")]
if remote_players.get(entity).is_ok() || server_enemies.get(entity).is_ok() { ... }
```

This eliminates `cfg(feature = "multiplayer")` from domain code entirely. Networking adds the marker when it creates entities. Domain code is feature-flag-free.

### Component Categories

Every component belongs to exactly one category:

| Category | Replicated? | Mutated by | Example |
|----------|------------|------------|---------|
| Authoritative | Yes | Resolve layer or server sync | Health, Position, Inventory |
| Cosmetic | Never | Presentation systems only | Particle timers, animation blend, HitFlash |
| Input | Client→Server | Input systems | Movement intent, attack request |
| Derived | Recomputed locally | Domain systems | Computed stat totals, UI display values |

Networking only touches Authoritative components. Cosmetic and Derived state never crosses the network.

## Rules System (Data-Driven Behaviors)

**Direction**: `docs/architecture/VISION.md`

The rules system enables data-driven reactive behaviors. The current implementation uses Rust enums for conditions/effects; the long-term plan is embedded Lua scripting over the same building blocks.

**Always prefer composing existing blocks over writing custom code.** When adding behaviors: first check if existing Stats, Conditions, Effects, and Triggers can do it. If not, add the smallest new building block. Never bypass the rules system with one-off observers.

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

**Development**
- `just` - Start SpacetimeDB, deploy module, run native dev build
- `just web` - Start SpacetimeDB, deploy module, run WASM dev server
- `just spacetimedb` - Only start SpacetimeDB and deploy module
- `just check` - Pre-commit checks: clippy, fmt, machete, web compilation check
- `just generate` - Regenerate SpacetimeDB bindings (also patches for WASM compatibility)

**Release**
- `just build` - Full release bundle (native → `dist/native/`, WASM → `dist/web/`)

**Testing**
- `cargo test` - Run all tests (currently minimal)

## Project Architecture

Bevy 0.18 3D action RPG targeting native and WebAssembly. Flat module architecture within each crate.

### Workspace

| Crate                     | Purpose                                                                                         |
|---------------------------|-------------------------------------------------------------------------------------------------|
| `client/`                 | Bevy game client — all gameplay, rendering, UI, audio                                           |
| `shared/`                 | Pure functions shared between client and server (combat resolution, rules, RNG). No Bevy types. |
| `server/`                 | SpacetimeDB module — authoritative game state, reducers                                         |
| `crates/spacetimedb-sdk/` | Local SpacetimeDB SDK fork (WASM compatibility patches)                                         |

### Major Systems (all under `client/src/`)

- **combat** — Attacks, damage, targeting, hit detection, hit feedback (screen shake, hit stop, damage numbers, VFX), enemy spawning, combat sounds
- **rules** — Data-driven behavior system via triggers and composable building blocks (stats, conditions, effects)
- **rule_presets** — Reusable rule compositions (crit, stacking buff)
- **player** — Character control (Tnua + Avian3d physics), animation state machine, footstep sounds
- **networking** — SpacetimeDB connection lifecycle, single reconciler that diffs server cache against ECS, interpolation, diagnostics resource for debug UI, local server management (native SP), world isolation (`world_id`)
- **camera** — Third-person orbit camera (Metin2-style, elevated pitch for combat visibility)
- **audio** — bevy_seedling (Firewheel) music and sound
- **scene** — Environment loading via bevy_skein (Blender workflow), skybox with day/night cycle
- **screens** — Screen state management (splash, loading, title, connecting, settings, gameplay), modal system
- **ui** — Reusable UI components: modals, settings panels, interaction observers
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

- `web` - Enables WebGPU backend for wasm32 target
- `dev` - Dev tools (inspector, debug UI). Default on; omitted for release builds
- `default` - Includes `dev` plus native-only features (dynamic linking, file/embedded watcher for hot-reloading)

### Naming

Full-length, explicit names. No abbreviations in struct fields, function names, component names, or resource names — except universally understood ones (`max`, `min`, `hp`, `id`, `fps`). Abbreviations are fine for very local variables (loop iterators, short-lived bindings).

- `animation_state` not `anim_state`
- `attack_sequence` not `attack_seq`
- `rotation_y` not `rot_y`
- `velocity_x` not `vel_x`

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
- Always regenerate SpacetimeDB bindings via `just generate`, never `spacetime generate` directly — the recipe patches WASM-incompatible methods (`advance_one_message_blocking`, `run_threaded`) that the codegen emits but our SDK fork removes

## Architecture Conventions

Reference: `docs/architecture/PATTERNS.md`

### Module Dependency Direction

Strict import hierarchy. Violations are bugs.

```
networking → combat, player, models, shared    (transport layer)
combat → shared, models                         (never imports networking)
player → shared, models, combat                 (may read combat components)
ui → models, combat, rules                      (reads state, never mutates)
models → (nothing game-specific)                (pure data definitions)
shared → (no Bevy types)                        (pure functions only)
```

Domain modules (combat, player) never import networking. If combat needs to tell the server something, it fires a domain event that networking observes.

### Event Flow

```
Request (imperative)  →  Resolve (shared/)  →  Outcome (past tense)  →  Feedback (cosmetic)
AttackIntent              resolve_combat()     DamageDealt              HitLanded
SpawnEnemyRequest         validate/position     EnemySpawned             (VFX, sound)
```

- **Requests**: imperative — `AttackIntent`, `SpawnEnemyRequest`. Hasn't happened yet.
- **Outcomes**: past tense — `DamageDealt`, `Died`. State changed.
- **Feedback**: past tense — `HitLanded`, `CritHit`. Cosmetic-only, terminal.

### Entity Spawning Ownership

The module that owns a domain concept owns its entity archetype.

- `combat/enemy.rs` owns enemy bundles (Health, Stats, Combatant, Collider, mesh)
- `player/` owns player bundles
- Networking should not construct domain bundles — it fires spawn events, the owning module's observer builds the entity

### SpacetimeDB

When making changes to SpacetimeDB server modules, client networking, or SDK code, read https://spacetimedb.com/llms.txt to ensure you're using the current API correctly.

The SDK at `crates/spacetimedb-sdk/` is a local fork with WASM patches (mutex safety, WebSocket split, credential storage). When upgrading SpacetimeDB versions, bump pins in both `server/Cargo.toml` and `crates/spacetimedb-sdk/Cargo.toml`, then verify the WASM patches still apply.

### Multiplayer Runtime

Every game session connects to SpacetimeDB. Native singleplayer launches a local subprocess; web solo connects to a remote server with a private `world_id`; multiplayer connects to a shared remote `world_id`. The server is the single source of truth for all modes.

`GameMode` resource (Singleplayer/Multiplayer) set by title screen buttons. `ServerTarget` resource (Local/Remote) describes where the SpacetimeDB instance lives. `GameMode` gates runtime behavior. Use `is_multiplayer_mode` run condition for MP-only systems.

## Rules System (Data-Driven Behaviors)

**Direction**: `docs/architecture/VISION.md`

The rules system enables data-driven reactive behaviors. The current implementation uses Rust enums for conditions/effects; the long-term plan is embedded Lua scripting over the same building blocks. Dynamic effects (buffs, debuffs, DoTs) use the `ActiveEffect` SpacetimeDB satellite table rather than hardcoded columns on entity tables.

**Always prefer composing existing blocks over writing custom code.** When adding behaviors: first check if existing Stats, Conditions, Effects, and Triggers can do it. If not, add the smallest new building block. Never bypass the rules system with one-off observers.

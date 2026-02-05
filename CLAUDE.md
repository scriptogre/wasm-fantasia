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

This is a Bevy 0.17 3D RPG game template targeting native and WebAssembly platforms. The codebase follows a flat architecture pattern with clear separation of concerns across modules.

### Core Module Structure

**`src/main.rs`** - App entrypoint. Plugin order matters: loads audio, asset_loading, ui, then game. Overrides default font and sets window icon.

**`src/models/`** - Data layer for the entire game. Contains:
- `input.rs` - EnhancedInput action definitions and bindings (PlayerCtx, ModalCtx). Uses mouse_motion for Pan, gamepad sticks, keyboard/gamepad buttons
- `states.rs` - GameState resource (pause, mute, diagnostics), Screen states enum, Mood enum (Exploration/Combat)
- `settings.rs` - Serializable Settings resource with volume controls
- `event_dispatch.rs` - Event routing system
- `player.rs` - Player component definition

**`src/asset_loading/`** - Handles centralized asset loading:
- `ron.rs` - Custom RON asset loader for Config/Credits
- `tracking.rs` - ResourceHandles resource that tracks loading progress
- Loads: Config, CreditsPreset from RON; Textures, Models as Resources; AudioSources (native only)

**`src/audio/`** - Built on bevy_seedling (Firewheel audio engine):
- `mod.rs` - MainBus volume setup, MusicPlaybacks HashMap tracking Mood->Entity
- `fade.rs` - Crossfade system for music transitions
- `radio.rs` - Radio playback system
- Only native; WASM has dependency conflicts with Firewheel

**`src/player/`** - Character control using Tnua + Avian3d physics:
- `control.rs` - Tnua movement system with walk, sprint, crouch, jump, dash observers
- `animation.rs` - Animation state machine syncing with Tnua state and attack state
- `sound.rs` - Footstep sound playback (native only)
- Spawned with: TnuaController, RigidBody::Dynamic, Capsule collider, ThirdPersonCameraTarget, combat components, rule_presets (crit, stacking)

**`src/combat/`** - Combat system (modular architecture):
- `components.rs` - Health, AttackState, Combatant markers, DamageEvent/DeathEvent/HitEvent
- `attack.rs` - Attack input handling, hit timing, AttackConnect event, spatial hit detection
- `damage.rs` - Damage application, knockback forces, death handling
- `targeting.rs` - Target locking system (LockedTarget component)
- `separation.rs` - Entity separation to prevent overlap
- `enemy.rs` - Test enemy spawning (E key)
- `hit_feedback.rs` - Juice effects: hit stop, screen shake, impact VFX, damage numbers
- `sound.rs` - Combat sound effects

**`src/rules/`** - Data-driven behavior system (see Rules System section below):
- `mod.rs` - Stats, Effects, Conditions, RuleVars, Op enum
- `triggers.rs` - Trigger components (OnHitRules, OnPreHitRules, etc.), observers, timer systems

**`src/rule_presets/`** - Reusable rule compositions:
- `crit.rs` - Critical hit system (chance, damage mult, knockback mult)
- `stacking.rs` - Stacking attack speed buff with inactivity decay

**`src/networking/`** - SpacetimeDB multiplayer:
- `mod.rs` - SpacetimeDB connection, config, plugin
- `player.rs` - Networked player sync
- `generated/` - Auto-generated SpacetimeDB bindings

**`src/postfx/`** - ReShade-style post-processing:
- Color grading with vibrance, contrast, shadows
- Toggle with F2

**`src/camera/`** - Third-person orbit camera (Metin2-style):
- `third_person.rs` - Uses bevy_third_person_camera with elevated pitch (~50°) for combat visibility
- Camera has: SceneCamera marker, Hdr, DeferredPrepass, TemporalAntiAliasing, Fxaa
- Mouse orbits around player, scroll to zoom

**`src/scene/`** - Environment loading:
- Uses bevy_skein for Blender->Bevy workflow (components added in Blender)
- `skybox.rs` - Day/night cycle with sun positioning
- Physics with avian3d

**`src/screens/`** - Screen state management:
- `splash.rs`, `loading.rs`, `title.rs`, `settings.rs`, `credits.rs`, `gameplay.rs`
- Modal system: Escape triggers Back navigation, stacked modals
- `to::*` module contains click observers for screen transitions

**`src/ui/`** - Reusable UI components:
- `modal.rs` - Modal spawning and back-navigation system
- `prefabs/` - Settings panels, keybinding editors, modals
- `interaction.rs` - Hover/click observers
- `constants.rs` - Game colors (primary, secondary, muted)

**`src/game/`** - Game mechanics:
- `dev_tools.rs` - Inspector egui (dev_native feature)
- `music.rs` - Music spawning with Mood components

### Input System (bevy_enhanced_input)

Actions are defined with `#[derive(InputAction)]` and bound via the `actions!` macro. Two contexts:
- `PlayerCtx` - Navigate (WASD/arrows/left stick), Pan (mouse/right stick), Jump, Sprint, Dash, Crouch, Attack, Pause, Mute, Escape
- `ModalCtx` - Navigate, Select, RightTab, LeftTab, Escape

Observer pattern: `On<Start<Action>>`, `On<Complete<Action>>`, `On<Fire<Action>>`

### Animation System

Uses Quaternius Universal Animation Library. AnimationState enum is synchronized with Tnua's state (standing, walking, running, jumping, falling, crouch). The animation is chosen based on Tnua's basis state and active actions.

### Physics (avian3d)

- Player is Dynamic RigidBody with capsule Collider
- TnuaAvian3dSensorShape for ground detection
- Friction::ZERO with Multiply combine rule
- Scene colliders loaded from GLTF via bevy_skein

### Feature Flags

- `web` - Enables wasm32 target
- `audio` - Enables bevy_seedling audio (native only)
- `third_person` - Orbit camera (default, required)
- `dev_native` - Dev tools, inspector, asset hot-reloading (native)

### Key Patterns

1. **System Sets**: `PostPhysicsAppSystems` in models/mod.rs defines ordering: UserInput -> TickTimers -> ChangeUi -> PlaySounds -> PlayAnimations -> Update

2. **Markers**: `PlayerCtx`, `ModalCtx`, `SceneCamera`, `DespawnOnExit` for query filtering

3. **Observers**: Heavily used for input handling (On<Start>, On<Complete>, On<Fire>), entity lifecycle (On<Add>, On<Remove>)

4. **Resources**: Config loaded from RON, Settings serializable, GameState tracks app-wide state, MusicPlaybacks tracks active music entities

5. **Camera Sync**: CameraSyncSet runs before TransformSystems::Propagate in PostUpdate

### Common Pitfalls

- Audio on web requires SharedArrayBuffer headers (COOP/COEP) or audio will stutter
- Hotpatching requires BEVY_ASSET_ROOT="." environment variable
- bevy_skein stores enum discriminants, not strings - library updates may break saved scenes
- Player rotation is locked on X/Z but free on Y (LockedAxes::ROTATION_LOCKED.unlock_rotation_y())
- TNUA movement is in FixedUpdate schedule, not Update
- Window icon uses WINIT_WINDOWS.with_borrow_mut directly due to Bevy bug #17667

## Rules System (Data-Driven Behaviors)

**Location**: `src/rules/` with presets in `src/rule_presets/`
**Documentation**: `docs/implementation/RULES_SYSTEM.md` (evolving - treat as reference, not gospel)

The rules system enables data-driven reactive behaviors without writing Rust code for each new mechanic. It follows a "lego block" philosophy.

### Core Philosophy

**Always prefer composing existing blocks over writing custom code.**

When implementing new gameplay behaviors:

1. **First**: Can this be done with existing Stats, Conditions, Effects, and Triggers?
2. **If not**: What's the smallest new building block needed? Add it to the rules system.
3. **Never**: Write one-off observers or systems that bypass the rules system for behavior that could be data-driven.

### Building Blocks (Smallest to Largest)

```
Stats (data)     → Conditions (if)   → Effects (then)    → Rules (if-then)
    ↓                   ↓                   ↓                   ↓
Triggers (when)  → Rule Components   → Presets (bundles) → Entities
```

| Layer | Examples | When to Add |
|-------|----------|-------------|
| **Stats** | `Stacks`, `DeltaTime`, `HitDamage` | Need to track new per-entity value |
| **Conditions** | `Gt`, `Lt`, `Lte`, `Chance` | Need new way to check stats |
| **Effects** | `Set`, `Modify`, `ModifyFrom`, `Damage` | Need new way to change stats |
| **Triggers** | `OnHitRules`, `OnTickRules`, `OnTimerRules` | Need new event to react to |
| **Presets** | `crit()`, `stacking()` | Common patterns worth reusing |

### Decomposition Process (How to Add New Mechanics)

When a feature doesn't fit existing blocks, decompose it into primitives:

**Example: "Inactivity Timer" (stacks reset after no hits)**

1. **What is it?** A countdown that resets on hit, triggers decay when expired.

2. **Break it down:**
   - Storage: just a float value → `Stat::Inactivity`
   - Decrement each frame: need frame delta → `Stat::DeltaTime` (system-provided)
   - Use one stat's value in effect → `Effect::ModifyFrom { stat, op, from }`
   - Check if expired: already have → `Condition::Lte(stat, 0.0)`

3. **Compose with existing blocks:**
   ```rust
   OnHitRules: Set Inactivity to 2.5 (reset countdown)
   OnTickRules: ModifyFrom { Inactivity, Subtract, DeltaTime } (decrement)
   OnTickRules: if Inactivity <= 0 && Stacks > 0, Set Stacks to 0
   ```

4. **No magic needed.** Timer behavior emerges from composition.

**Key questions when decomposing:**
- What data does this need? → Stats
- What changes the data? → Effects
- What reads the data? → Conditions
- When does it happen? → Triggers
- Can existing blocks do this? If not, what's the minimal new block?

### Anti-Patterns

```rust
// DON'T: Magic arrays marking certain stats as special
const TIMER_STATS: &[Stat] = &[...];

// DON'T: Separate storage for "different kinds" of the same thing
struct RuleTimers(HashMap<Timer, f32>);  // Just use RuleVars

// DON'T: Custom observers bypassing rules
fn my_special_on_hit_observer(...) { /* directly modifies stuff */ }
```

### Documentation Status

`docs/implementation/RULES_SYSTEM.md` documents the current state but is under active development. If you find a cleaner implementation approach:

1. Implement it properly within the rules system architecture
2. Update the documentation to match
3. The code is the source of truth, not the docs

### Key Files

- `src/rules/mod.rs` - Stats, Effects, Conditions, RuleVars, Op
- `src/rules/triggers.rs` - Trigger components (OnHitRules, etc.), observers, timer systems
- `src/rule_presets/*.rs` - Composed presets (crit, stacking)

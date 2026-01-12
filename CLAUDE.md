# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

**Native Development**
- `make run` or `cargo run` - Run native dev build
- `make build` - Build native release
- `make lint` - Run clippy, fmt check, and machete (unused deps)
- `make hot` or `dx serve --hot-patch` - Run with hotpatching (requires BEVY_ASSET_ROOT="." on Linux/macOS)

**Web Development**
- `make run-web` or `bevy run web --headers="Cross-Origin-Opener-Policy:same-origin" --headers="Cross-Origin-Embedder-Policy:credentialless"` - Run web dev with SharedArrayBuffer support (required for audio)
- `make build-web` - Build web release bundle
- `make check-web` - Quick check web compilation without full build

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
- `control.rs` - Tnua movement system with walk, sprint, crouch, jump, dash observers. Updates StepTimer based on actual velocity
- `animation.rs` - Animation state machine syncing with Tnua state
- `sound.rs` - Footstep sound playback (native only)
- Spawned with: TnuaController, LockedAxes::ROTATION_LOCKED (Y-unlocked), RigidBody::Dynamic, Capsule collider, ThirdPersonCameraTarget/TopDownCameraTarget

**`src/camera/`** - Camera modes via feature flags:
- `third_person.rs` - Uses bevy_third_person_camera plugin
- `top_down.rs` - Uses bevy_top_down_camera plugin
- Camera has: SceneCamera marker, Hdr, DeferredPrepass, TemporalAntiAliasing, Fxaa

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
- `third_person` / `top_down` - Camera modes (default: third_person)
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

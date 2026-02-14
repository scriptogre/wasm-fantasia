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

### Animation Pipeline

`player.source.glb` contains the full Quaternius animation library. `build.rs` parses `Animation::clip_name()` and generates an optimized `player.glb` with only registered clips. To add an animation: add the enum variant and its clip_name/from_clip_name mappings — the build pipeline handles the rest.

### Feature Flags

- `web` - Enables WebGPU backend for wasm32 target
- `dev` - Dev tools (inspector, debug UI). Default on; omitted for release builds
- `default` - Includes `dev` plus native-only features (dynamic linking, file/embedded watcher for hot-reloading)

### Multiplayer Runtime

Every game session connects to SpacetimeDB. Native singleplayer launches a local subprocess; web solo connects to a remote server with a private `world_id`; multiplayer connects to a shared remote `world_id`. The server is the single source of truth for all modes.

`GameMode` resource (Singleplayer/Multiplayer) set by title screen buttons. `ServerTarget` resource (Local/Remote) describes where the SpacetimeDB instance lives. `GameMode` gates runtime behavior. Use `is_multiplayer_mode` run condition for MP-only systems.

## Rules System (Data-Driven Behaviors)

**Direction**: `docs/architecture/VISION.md`

The rules system enables data-driven reactive behaviors. The current implementation uses Rust enums for conditions/effects; the long-term plan is embedded Lua scripting over the same building blocks. Dynamic effects (buffs, debuffs, DoTs) use the `ActiveEffect` SpacetimeDB satellite table rather than hardcoded columns on entity tables.

**Always prefer composing existing blocks over writing custom code.** When adding behaviors: first check if existing Stats, Conditions, Effects, and Triggers can do it. If not, add the smallest new building block. Never bypass the rules system with one-off observers.

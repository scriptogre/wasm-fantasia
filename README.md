# wasm-fantasia

Session-based MMO prototype. Bevy 0.17 + SpacetimeDB multiplayer, targeting native and WebAssembly.

**Status:** Early prototype. Expect rough edges.

## What's here

- 3D character controller (Tnua + Avian3d physics)
- Third-person orbit camera with gamepad support
- Combat system with attacks, targeting, damage numbers, hit VFX, screen shake
- Data-driven rules engine (stats, conditions, effects, triggers)
- SpacetimeDB multiplayer with auto-reconnect, session persistence, server status HUD
- Network lag/packet-loss simulator for testing
- Day/night skybox cycle
- Audio system with music crossfading (native only)
- Screen flow: splash, loading, title, settings, gameplay
- Blender scene integration via bevy_skein

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [just](https://github.com/casey/just) (command runner)
- [Bevy CLI](https://github.com/TheBevyFlock/bevy_cli) (for web builds)
- [SpacetimeDB](https://spacetimedb.com/install) (for multiplayer, auto-installed by `just`)
- Linux users: install [Bevy's Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)

## Getting started

```bash
just              # Start SpacetimeDB, deploy module, run native dev build
just web          # Start SpacetimeDB, deploy module, run WASM dev server
just spacetimedb  # Only start SpacetimeDB and deploy module
just build        # Native release build
just web-build    # Full web release bundle
just check        # Clippy + fmt + machete + web compilation check
```

## Project structure

| Path | Description |
|------|-------------|
| `client/` | Bevy game client — all gameplay, rendering, UI, audio |
| `shared/` | Pure functions shared between client and server (combat, rules, RNG) |
| `server/` | SpacetimeDB server module — authoritative game state, reducers |
| `crates/` | Local dependency forks (spacetimedb-sdk, tokio-tungstenite-wasm) |
| `docs/` | Design and architecture documents |

## Feature flags

| Flag | Description |
|------|-------------|
| `dev_native` | Dev tools, inspector, asset hot-reloading (default) |
| `audio` | bevy_seedling audio, native only (included in `dev_native`) |
| `third_person` | Third-person orbit camera (default) |
| `multiplayer` | SpacetimeDB networking |
| `web` | WebGPU/WASM target |

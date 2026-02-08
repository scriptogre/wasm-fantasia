# wasm-fantasia

Session-based MMO prototype. Bevy 0.17 + SpacetimeDB multiplayer, targeting native and WebAssembly.

**Status:** Early prototype. Expect rough edges.

## What's here

- 3D character controller (Tnua + Avian3d physics)
- Third-person camera with gamepad support
- SpacetimeDB multiplayer with server-authoritative movement
- Network lag/packet-loss simulator for testing (F1-F7)
- Day/night skybox cycle
- Audio system with music crossfading (native only)
- Screen flow: splash, loading, title, settings, gameplay
- Blender scene integration via bevy_skein

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [just](https://github.com/casey/just) (command runner)
- [Bevy CLI](https://github.com/TheBevyFlock/bevy_cli) (for web builds)
- [SpacetimeDB](https://spacetimedb.com/install) (for multiplayer, auto-installed by `just mp`)
- Linux users: install [Bevy's Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)

## Getting started

```bash
just              # Run native dev build
just mp           # Start SpacetimeDB server + two clients
just build        # Native release build
just check        # Clippy + fmt + machete + web compilation check
just web          # Run WASM dev server
just web-mp       # Start SpacetimeDB server + WASM dev server
just web-build    # Full web release bundle
```

### SpacetimeDB server

```bash
cd server
spacetimedb publish wasm-fantasia
```

## Project structure

| Path | Description |
|------|-------------|
| `src/main.rs` | App entrypoint, plugin registration |
| `src/models/` | Data layer: input bindings, game state, settings |
| `src/player/` | Character control, animation, footstep sounds |
| `src/camera/` | Third-person / top-down camera (feature-flagged) |
| `src/scene/` | Environment loading, skybox |
| `src/audio/` | Music bus, crossfading, radio (native only) |
| `src/networking/` | SpacetimeDB client, position sync, lag simulator |
| `src/screens/` | Splash, loading, title, settings, gameplay |
| `src/ui/` | Reusable UI components, modals |
| `src/game/` | Game mechanics, dev tools |
| `server/` | SpacetimeDB server module |
| `docs/` | Design documents |

## Feature flags

| Flag | Description |
|------|-------------|
| `dev_native` | Dev tools, inspector, asset hot-reloading (default) |
| `audio` | bevy_seedling audio, native only (included in `dev_native`) |
| `third_person` | Third-person camera (default) |
| `top_down` | Top-down camera |
| `web` | WebGPU/WASM target |

## Credits

Assets are all third-party. See [credits](assets/credits.json).

## License

Source code is licensed under any of:
- [CC0-1.0](./LICENSE-CC0)
- [MIT](./LICENSE-MIT)
- [Apache 2.0](./LICENSE-APACHE)

Based on [bevy_new_3d_rpg](https://github.com/olekspickle/bevy_new_3d_rpg) by Oleks Pickle.

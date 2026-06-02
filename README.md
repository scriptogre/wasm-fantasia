# WASM Fantasia

Session-based MMO prototype. Bevy 0.17 + SpacetimeDB multiplayer, targeting native and WebAssembly.

<img width="500" alt="wasm fantasia" src="https://github.com/user-attachments/assets/e1182eb1-bc7b-43da-93ab-e51b36979e69" />

**Status:** Very early prototype.

## What's here

- 3D character controller (Tnua + Avian3d physics)
- Third-person orbit camera with gamepad support
- Combat system with attacks, targeting, damage numbers, hit VFX, screen shake
- Animated zombie enemies with chase-and-attack AI (server-side)
- Data-driven rules engine (stats, conditions, effects, triggers)
- Self-contained native distribution (`just build`)

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [just](https://github.com/casey/just) (command runner)
- [Bevy CLI](https://github.com/TheBevyFlock/bevy_cli) (for web builds)
- [SpacetimeDB](https://spacetimedb.com/install) (required — all modes connect to SpacetimeDB)
- Linux users: install [Bevy's Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)

## Getting started

```bash
just              # Start SpacetimeDB, deploy module, run native dev build
just web          # Start SpacetimeDB, deploy module, run WASM dev server
just spacetimedb  # Only start SpacetimeDB and deploy module
just build        # Native release bundle (dist/native/)
just web-build    # Full web release bundle
just check        # Clippy + fmt + machete + web compilation check
```

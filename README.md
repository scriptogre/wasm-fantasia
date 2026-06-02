# WASM Fantasia

A multiplayer horde-combat prototype built on Bevy and SpacetimeDB, running native and in the browser.

<img width="500" alt="wasm fantasia" src="https://github.com/user-attachments/assets/e1182eb1-bc7b-43da-93ab-e51b36979e69" />

Bevy 0.18 + SpacetimeDB, native and WebAssembly. Early prototype, aiming toward a session-based action roguelite.

## What's here

- Massive enemy hordes (VAT-animated) with server-side AI
- Crunchy combat: cone auto-target, damage numbers, hit VFX, screen shake
- Wave survival with death and restart
- Rune scripting runtime for data-driven abilities, behaviors, and rules
- 3D character controller (Tnua + Avian3d) and third-person camera with gamepad
- SpacetimeDB as the authority for every mode (singleplayer runs a local instance)

Design notes live in [`docs/design/`](docs/design/).

## Run it

Needs [Rust](https://rustup.rs/), [just](https://github.com/casey/just), [Bevy CLI](https://github.com/TheBevyFlock/bevy_cli) (web builds), and [SpacetimeDB](https://spacetimedb.com/install). On Linux, also [Bevy's deps](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md).

```bash
just         # native dev build
just web     # WASM dev server
just build   # release bundles (dist/)
just check   # lint + web compile check
```

Run `just --list` for the rest.

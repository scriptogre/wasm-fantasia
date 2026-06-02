# CLAUDE.md

## Build Commands

See `Justfile` for all available commands. Run `just --list` to see them.

## Project

Bevy 3D game targeting native and WebAssembly (equal priority). Always target the latest Bevy release.

## Design

Vision and architecture live in `docs/design/`: [GAMEPLAY](docs/design/GAMEPLAY.md), [AESTHETIC](docs/design/AESTHETIC.md), [ARCHITECTURE](docs/design/ARCHITECTURE.md). Read them before designing features.

## Code Standards

- **Idiomatic Rust, idiomatic Bevy.** Always check the latest Bevy docs and use current APIs and patterns.
- **Think before writing.** Consider whether a new feature warrants a new system, a new component, an event, or an extension of existing logic. Do not blindly add parameters to existing functions to handle new edge cases — that is not a solution.
- **Elegance and simplicity.** Every addition should feel like it belongs. No hacky workarounds. If something feels forced, step back and reconsider the approach.
- **No proactive refactoring.** Don't restructure working code unless asked. Fix what you're asked to fix, build what you're asked to build.

## Fix Discipline

- **Never claim something is fixed until the user confirms it.** Say "this should address it — try it out" not "fixed." You don't know it's fixed until it's tested.
- **Revert failed attempts immediately.** If a fix doesn't work, remove it completely before trying something else. Don't leave dead-weight code from failed attempts in the codebase.
- **One hypothesis at a time.** Don't stack speculative changes. Make one change, have it tested, then iterate.
- **If you don't know the root cause, say so.** Don't guess-and-ship. Investigate first, or ask the user to provide more info (logs, screenshots, reproduction steps).

## Performance Discipline

This is a real-time game targeting 60fps with thousands of entities. **Every per-frame system has a performance cost.** When implementing new features:

- Run `/performance` after adding systems, especially those touching many entities
- Rune scripts are slower than direct Rust — avoid running scripts per-entity per-frame when possible. Prefer event-driven script execution (on hit, on spawn, on wave change) over polling
- Batch operations: spawn with `spawn_batch`, process in bulk, avoid per-entity allocations
- Shared material/mesh handles — never allocate per entity (see `/performance` GPU checklist)
- Gate update systems with run conditions (`Changed<T>`, `on_event`, state checks) so they don't run when there's nothing to do
- WASM target makes heap allocation 10-20x more expensive — minimize String clones and unnecessary Vec allocations

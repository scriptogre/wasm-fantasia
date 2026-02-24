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

| Crate                                           | Purpose                                                                                  |
|-------------------------------------------------|------------------------------------------------------------------------------------------|
| `client/` (`game-client`)                       | Bevy game client — all gameplay, rendering, UI, audio                                    |
| `client/models/` (`game-client-models`)         | Bevy-specific types: components, resources, states, events, input, animation, theme      |
| `client/networking/` (`game-client-networking`) | SpacetimeDB SDK integration, connection, sync, reconciliation, generated bindings        |
| `core/` (`game-core`)                           | Pure functions shared between client and server (combat resolution, RNG). No Bevy types. |
| `server/` (`game-server`)                       | SpacetimeDB module — authoritative game state, reducers                                  |

### Animation Pipeline

`player.source.glb` contains the full Quaternius animation library. `build.rs` parses `Animation::clip_name()` and generates an optimized `player.glb` with only registered clips. To add an animation: add the enum variant and its clip_name/from_clip_name mappings — the build pipeline handles the rest.

### Feature Flags

- `web` - Enables WebGPU backend for wasm32 target
- `dev` - Dev tools (inspector, debug UI). Default on; omitted for release builds
- `default` - Includes `dev` plus native-only features (dynamic linking, file/embedded watcher for hot-reloading)

### Multiplayer Runtime

Every game session connects to SpacetimeDB. Native singleplayer launches a local subprocess; web solo connects to a remote server with a private `world_id`; multiplayer connects to a shared remote `world_id`. The server is the single source of truth for all modes.

`GameMode` resource (Singleplayer/Multiplayer) set by title screen buttons. `ServerTarget` resource (Local/Remote) describes where the SpacetimeDB instance lives. `GameMode` gates runtime behavior. Use `is_multiplayer_mode` run condition for MP-only systems.

## Active Work: GPU Profiling & Enemy Performance (2026-02-24)

### Context
With 10K VAT enemies on screen, frame time is ~50-63ms (15-20 FPS). We need to identify whether the bottleneck is CPU or GPU to know what to optimize.

### What was built
- **GPU pass profiler** (`client/src/gpu_pass_profiler.rs`): Uses wgpu timestamp queries to measure per-render-pass GPU time. Replaces the old phase-based F10 profiler. Press F10 to record 10s, prints per-pass timing to terminal.
- **ClipId refactor** in bevy_open_vat: Changed `VatAnimationController.current_clip` from `String` to `u8` clip ID.

### What we learned
1. **GPU timestamp queries are unreliable on Metal (Apple Silicon)**: The "full_frame" span reports 1.4ms while individual passes sum to 4.1ms — physically impossible. `CommandEncoder::write_timestamp` doesn't produce correctly ordered timestamps across render passes on Metal.
2. **F9 CPU profiler shows `prepare_windows` at 38ms (77% of frame)**: This system calls `surface.get_current_texture()` and blocks until the GPU finishes the previous frame. This is the most reliable indicator that the GPU is busy for ~38ms.
3. **The old phase-based profiler was actually more useful on Metal**: It measured real frame time impact by toggling scene elements (hide enemies, disable shadows, etc.). While it can't distinguish CPU vs GPU cost, it correctly showed "enemies cost ~55ms total."
4. **A compute pre-skinning approach was attempted and reverted** — it optimized VAT texture fetches, but texture fetches weren't the bottleneck. The design doc assumed wrong; we should have profiled first.

### Next steps (pick up here)
1. **Bring back old phase-based profiler alongside the new one** (e.g., old=F10, new=F11) — the phase-based approach gives actionable data on Metal even though it's crude.
2. **Determine CPU vs GPU split**: The 38ms `prepare_windows` suggests GPU-bound, but F9 also shows significant CPU work (`run_fixed_main_schedule`: 10.8ms, `handle_connection_events`: 5.4ms, physics: 1.8ms). Need to figure out which dominates.
3. **Once bottleneck is identified**, potential optimizations:
   - If GPU-bound: aggressive LOD (current LOD1 is 25% triangles, could go to 5-10%), GPU frustum culling, reduce draw calls
   - If CPU-bound: reduce ECS entity count, batch enemy processing, optimize `collect_meshes_for_gpu_building`

### Key files
- `client/src/gpu_pass_profiler.rs` — new GPU timestamp profiler (unreliable on Metal)
- `client/src/gpu_profiler.rs` — old phase-based profiler (DELETED, but in git history at commit `be481ad~1`)
- `client/src/ui/performance.rs` — F9 CPU benchmark + stats overlay
- `client/src/profiling.rs` — per-system CPU tracing layer
- `docs/plans/2026-02-24-gpu-pass-profiler-design.md` — design doc
- `docs/plans/2026-02-24-compute-preskinning-design.md` — failed optimization attempt (reverted)

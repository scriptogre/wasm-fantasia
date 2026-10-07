# Gameplay plugins

Fury is a Rust WebAssembly component prototype. The game uses its shared Rust implementation directly; it does not load this plugin yet.

## Contract

[wit/world.wit](wit/world.wit) defines `fantasia:mechanics/fury@0.1.0`. `wit-bindgen` generates the Rust interface from it.

- Inputs: stack count, hit flags and elapsed microseconds.
- Outputs: updated state and attack-speed bonus.
- State belongs to the host. The component has no imports or ambient I/O.
- [fury/src/lib.rs](fury/src/lib.rs) adapts the [verified implementation](../core/src/fury.rs). Keep the formula there once.
- [Fury laws](../core/verification/README.md) cover that implementation. They do not prove the adapter, allocator or host.

This interface covers Fury only. Skills, items and enemy definitions still need a common content contract. `probe/` is a test host, without production resource limits or state migration.

## Run

Requires Rust 1.99, Bun, Just and Chromium. Set `CHROMIUM` to its executable if needed.

```sh
rustup target add --toolchain stable wasm32-wasip2 wasm32-unknown-unknown
cd plugins
just benchmark
```

The benchmark checks outputs, expiry and instance replacement before timing. It writes [benchmark-result.json](benchmark-result.json). Setup is timed separately. Each result is the median of nine rotated rounds after warmup.

`just server` builds a SpacetimeDB test module at `target/server-probe.wasm`. Its `verify_component(bytes)` reducer loads a component and checks 17 cases against linked Rust.

## Measurements

ThinkCentre i5-13500T, Chromium 151, release builds. Milliseconds per 1,000 Fury hits, lower is better:

| Path | 1 hit/call | 8 hits/call | 64 hits/call |
|---|---:|---:|---:|
| Rune 0.14.1 | 1.159 | 1.163 | 1.148 |
| Component via nested Wasmi | 2.794 | 0.919 | 0.349 |
| Component via browser JIT | 2.128 | 0.382 | 0.078 |
| Linked Rust | 0.006 | 0.007 | 0.004 |

Component paths include state transfer and a second call for the bonus. Batched paths amortize that cost. These measure Fury, not whole attacks or frame rate.

The Rune fixture is the original stacking script from commit `29f20b8`, kept only as a benchmark baseline. Valid stack inputs match; invalid-input checks apply to the Rust paths because the original script did not clamp them.

The browser JIT path uses [jco](https://component-model.bytecodealliance.org/language-support/building-a-simple-component/javascript.html). The nested path uses `wasm_component_layer` 0.1.18 and Wasmi 0.31.2. Keep `Cargo.lock`: the component adapter requires runtime-layer 0.4.2. Newer Wasmi engines are not measured.

## Rune coverage

Five scripts exist: melee, ground pound, critical hits, stacking and feedback. The first four run by default; feedback is registered but unattached. Enemy AI runs in Rust. No full Rune migration is implemented.

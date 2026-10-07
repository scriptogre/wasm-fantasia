For gameplay changes, follow the [Rust + Verus guide](https://github.com/scriptogre/law-driven-rust/blob/cbe654543b7ed29d79ebf513aec47d1826675140/GUIDE.md).

- Keep `core/verification/laws.rs`, `verify.rs` and the full verification command fixed during implementation. Ask the user before changing law meaning or input domains.
- Run `just verify` and the affected runtime tests before committing.
- Verify the executable source used by the game. Keep one implementation across native, WASM and plugin builds.

# Fury laws

Normal hits add 1 stack; critical hits add 3. Stacks cap at 12 and grant 12% attack speed each. A hit refreshes the full 2.5-second lifetime. Negative counts start at zero.

| File | Purpose |
|---|---|
| [laws.rs](laws.rs) | Required contracts |
| [proof.rs](proof.rs) | Proof bodies |
| [verify.rs](verify.rs) | Requires every proof and checks the full input domain |
| [../src/fury.rs](../src/fury.rs) | The implementation compiled into the game and plugin |

Use Verus `0.2026.10.04.426d8b0` with its Rust `1.98.1` toolchain:

```sh
VERUS=/path/to/verus just verify
cargo test -p game-core
```

The gate checks all `i64` counts and `u64` times, then rejects eight broken variants. Cargo excludes only the verification annotations; the executable bodies stay identical.

Proofs cover Fury's integer transitions. Engine clocks, float conversion, Rune hooks, networking and plugin adapters need runtime tests. Other mechanics have no Verus guarantees.

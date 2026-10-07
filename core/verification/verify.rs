#[path = "../src/fury.rs"]
mod fury;
mod laws;
mod proof;

use vstd::prelude::*;

verus! {
proof fn require_laws<T: laws::Laws>() {}

proof fn check_laws() {
    require_laws::<proof::Proofs>();
}

fn check_input_domain(stacks: i64, is_crit: bool, remaining_micros: u64, micros: u64) {
    let _ = fury::bounded(stacks);
    let _ = fury::on_hit(stacks, is_crit);
    let _ = fury::bonus_percent(stacks);
    let _ = fury::elapse(fury::Fury { stacks, remaining_micros }, micros);
}
}

fn main() {}

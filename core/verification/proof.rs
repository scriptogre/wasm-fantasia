use crate::{fury::Fury, laws::Laws};
use vstd::prelude::*;

verus! {
pub struct Proofs;

impl Laws for Proofs {
    proof fn hit(stacks: i64, is_crit: bool) {}
    proof fn bonus(stacks: i64) {}
    proof fn lifetime(state: Fury, micros: u64) {}
    proof fn refresh(stacks: i64, is_crit: bool, micros: u64) {}
}
}

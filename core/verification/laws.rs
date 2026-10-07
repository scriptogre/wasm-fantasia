use crate::fury::{self, Fury};
use vstd::prelude::*;

verus! {
pub trait Laws {
    proof fn hit(stacks: i64, is_crit: bool)
        ensures ({
            let start: int = if stacks < 0 { 0 } else if stacks > 12 { 12 } else { stacks as int };
            let sum = start + if is_crit { 3int } else { 1int };
            let result = fury::on_hit(stacks, is_crit);
            result.stacks == if sum > 12 { 12 } else { sum }
                && result.remaining_micros == 2_500_000
        });

    proof fn bonus(stacks: i64)
        ensures fury::bonus_percent(stacks) == 12 * (
            if stacks < 0 { 0int } else if stacks > 12 { 12int } else { stacks as int }
        );

    proof fn lifetime(state: Fury, micros: u64)
        ensures ({
            let remaining = if state.remaining_micros > 2_500_000 { 2_500_000u64 }
                else { state.remaining_micros };
            let result = fury::elapse(state, micros);
            if micros >= remaining || state.stacks <= 0 {
                result.stacks == 0 && result.remaining_micros == 0
            } else {
                result.stacks == if state.stacks > 12 { 12 } else { state.stacks }
                    && result.remaining_micros == remaining - micros
            }
        });

    proof fn refresh(stacks: i64, is_crit: bool, micros: u64)
        ensures ({
            let hit = fury::on_hit(stacks, is_crit);
            let result = fury::elapse(hit, micros);
            if micros >= 2_500_000 {
                result.stacks == 0 && result.remaining_micros == 0
            } else {
                result.stacks == hit.stacks && result.remaining_micros == 2_500_000 - micros
            }
        });
}
}

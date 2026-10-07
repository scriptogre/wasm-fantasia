#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fury {
    pub stacks: i64,
    pub remaining_micros: u64,
}

#[cfg_attr(verus_keep_ghost, verus_verify(dual_spec, open))]
#[cfg_attr(verus_keep_ghost, verus_spec(returns bounded(stacks)))]
#[allow(clippy::manual_clamp, reason = "Verus uses these branches as the logical definition.")]
pub fn bounded(stacks: i64) -> i64 {
    if stacks < 0 {
        0
    } else if stacks > 12 {
        12
    } else {
        stacks
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify(dual_spec, open))]
#[cfg_attr(verus_keep_ghost, verus_spec(returns on_hit(stacks, is_crit)))]
pub fn on_hit(stacks: i64, is_crit: bool) -> Fury {
    let gain = if is_crit { 3 } else { 1 };
    Fury {
        stacks: bounded(bounded(stacks) + gain),
        remaining_micros: 2_500_000,
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify(dual_spec, open))]
#[cfg_attr(verus_keep_ghost, verus_spec(returns elapse(state, micros)))]
pub fn elapse(state: Fury, micros: u64) -> Fury {
    let remaining = if state.remaining_micros > 2_500_000 {
        2_500_000
    } else {
        state.remaining_micros
    };
    if micros >= remaining || state.stacks <= 0 {
        Fury {
            stacks: 0,
            remaining_micros: 0,
        }
    } else {
        Fury {
            stacks: bounded(state.stacks),
            remaining_micros: remaining - micros,
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify(dual_spec, open))]
#[cfg_attr(verus_keep_ghost, verus_spec(returns bonus_percent(stacks)))]
pub fn bonus_percent(stacks: i64) -> u16 {
    (bounded(stacks) * 12) as u16
}

//! Stacking attack speed buff preset (Jax-style)

use crate::rules::{OnCritHitRules, OnHitRules, OnTickRules};
use bevy::prelude::*;
pub use wasm_fantasia_shared::presets::stacking::StackingConfig;

pub fn stacking(config: StackingConfig) -> impl Bundle {
    let r = wasm_fantasia_shared::presets::stacking::stacking_rules(config);
    (
        OnHitRules(r.on_hit),
        OnCritHitRules(r.on_crit_hit),
        OnTickRules(r.on_tick),
    )
}

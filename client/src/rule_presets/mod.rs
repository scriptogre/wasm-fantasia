//! Rule Presets - Bevy component wrappers around shared EntityRules.

pub mod feedback;

use crate::rules::*;
use bevy::prelude::*;
use game_core::presets::EntityRules;

/// Convert shared EntityRules into a Bevy component bundle.
pub fn rules_bundle(rules: EntityRules) -> impl Bundle {
    (
        OnPreHitRules(rules.pre_hit),
        OnHitRules(rules.on_hit),
        OnCritHitRules(rules.on_crit_hit),
        OnTickRules(rules.on_tick),
        OnKillRules(rules.on_kill),
        OnTakeDamageRules(rules.on_take_damage),
    )
}

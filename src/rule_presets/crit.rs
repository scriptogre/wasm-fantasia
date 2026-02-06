//! Critical hit system preset
//!
//! Crits boost damage, knockback, and feedback intensity.
//! Required Stats: CritChance, CritMultiplier

use bevy::prelude::*;

use super::feedback;
use crate::rules::{
    action, stat, ActionVar, Condition, Effect, Expr, OnPreHitRules, Rule, RuleEvent, Stat,
};

/// Feedback amplification for crits
const CRIT_FEEDBACK_MULT: f32 = 2.5;

pub fn crit() -> impl Bundle {
    let mut rule = Rule::new()
        .when(Condition::Chance(stat(Stat::CritChance)))
        // Trigger crit event (caller will handle)
        .then(Effect::Trigger(RuleEvent::Crit))
        // Boost damage by CritMultiplier
        .then(Effect::SetAction {
            var: ActionVar::Damage,
            value: Expr::Multiply(
                Box::new(action(ActionVar::Damage)),
                Box::new(stat(Stat::CritMultiplier)),
            ),
        })
        // Boost knockback by CritMultiplier
        .then(Effect::SetAction {
            var: ActionVar::Knockback,
            value: Expr::Multiply(
                Box::new(action(ActionVar::Knockback)),
                Box::new(stat(Stat::CritMultiplier)),
            ),
        });

    // Amplify feedback using the preset
    for effect in feedback::amplify(CRIT_FEEDBACK_MULT) {
        rule = rule.then(effect);
    }

    OnPreHitRules(vec![rule])
}

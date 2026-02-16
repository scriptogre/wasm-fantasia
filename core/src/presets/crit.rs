//! Critical hit rule construction.

use crate::presets::feedback;
use crate::rules::{ActionVar, Condition, Effect, Expr, Rule, RuleEvent, Stat, action, stat};

pub const CRIT_FEEDBACK_MULT: f32 = 2.5;

/// Construct the crit rules (OnPreHitRules content).
pub fn crit_rules() -> Vec<Rule> {
    let mut rule = Rule::new()
        .when(Condition::Chance(stat(Stat::CritChance)))
        .then(Effect::Trigger(RuleEvent::Crit))
        .then(Effect::SetAction {
            var: ActionVar::Damage,
            value: Expr::Multiply(
                Box::new(action(ActionVar::Damage)),
                Box::new(stat(Stat::CritMultiplier)),
            ),
        })
        .then(Effect::SetAction {
            var: ActionVar::Knockback,
            value: Expr::Multiply(
                Box::new(action(ActionVar::Knockback)),
                Box::new(stat(Stat::CritMultiplier)),
            ),
        });

    for effect in feedback::amplify(CRIT_FEEDBACK_MULT) {
        rule = rule.then(effect);
    }

    vec![rule]
}

//! Critical hit system preset

use bevy::prelude::*;

use crate::rules::{OnPreHitRules, Rule, RuleVars, Var::*};

#[derive(Clone, Debug)]
pub struct CritConfig {
    pub crit_rate: f32,
    pub damage_mult: f32,
    pub knockback_mult: f32,
}

impl Default for CritConfig {
    fn default() -> Self {
        Self {
            crit_rate: 0.20,
            damage_mult: 2.5,
            knockback_mult: 2.5,
        }
    }
}

pub fn crit(config: CritConfig) -> impl Bundle {
    (
        RuleVars::new().with(CritRate, config.crit_rate),
        OnPreHitRules(vec![Rule::new()
            .when(CritRate.roll())
            .then(IsCrit.set_true())
            .then(HitDamage.set(HitDamage * config.damage_mult))
            .then(HitForceRadial.set(HitForceRadial * config.knockback_mult))]),
    )
}

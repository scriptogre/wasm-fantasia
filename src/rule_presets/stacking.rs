//! Stacking attack speed buff preset (Jax-style)

use bevy::prelude::*;

use crate::rules::{OnCritHitRules, OnHitRules, OnTickRules, Rule, Var::*};

/// Configuration for the stacking buff
#[derive(Clone, Debug)]
pub struct StackingConfig {
    pub gain_per_hit: f32,
    pub crit_bonus: f32,
    pub max_stacks: f32,
    pub decay_interval: f32,
}

impl Default for StackingConfig {
    fn default() -> Self {
        Self {
            gain_per_hit: 1.0,
            crit_bonus: 2.0,
            max_stacks: 12.0,
            decay_interval: 2.5,
        }
    }
}

pub fn stacking(config: StackingConfig) -> impl Bundle {
    let max = config.max_stacks;

    (
        OnHitRules(vec![
            Rule::new()
                .then(Stacks.set(Stacks + config.gain_per_hit))
                .then(Inactivity.set(config.decay_interval)),
            Rule::new()
                .when(Stacks.greater_than(max))
                .then(Stacks.set(max)),
        ]),
        OnCritHitRules(vec![
            Rule::new().then(Stacks.set(Stacks + config.crit_bonus)),
            Rule::new()
                .when(Stacks.greater_than(max))
                .then(Stacks.set(max)),
        ]),
        OnTickRules(vec![
            // Inactivity = Inactivity - DeltaTime
            Rule::new()
                .when(Inactivity.greater_than(0.0))
                .then(Inactivity.set(Inactivity - DeltaTime)),
            // When expired, reset stacks
            Rule::new()
                .when(Inactivity.less_than_or_equal(0.0))
                .when(Stacks.greater_than(0.0))
                .then(Stacks.set(0.0)),
        ]),
    )
}

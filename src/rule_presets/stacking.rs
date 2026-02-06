//! Stacking attack speed buff preset (Jax-style)

use bevy::prelude::*;

use crate::rules::{
    action, stat, val, ActionVar, Condition, Effect, Expr, OnCritHitRules, OnHitRules,
    OnTickRules, Rule, Stat,
};

// System-specific stats (stored in entity Stats using Custom)
fn stacks_stat() -> Stat {
    Stat::Custom("Stacks".into())
}

fn decay_stat() -> Stat {
    Stat::Custom("StackDecay".into())
}

/// Configuration for the stacking buff
#[derive(Clone, Debug)]
pub struct StackingConfig {
    pub gain_per_hit: f32,
    pub crit_bonus: f32,
    pub max_stacks: f32,
    pub decay_interval: f32,
    pub speed_per_stack: f32,
}

impl Default for StackingConfig {
    fn default() -> Self {
        Self {
            gain_per_hit: 1.0,
            crit_bonus: 2.0,
            max_stacks: 12.0,
            decay_interval: 2.5,
            speed_per_stack: 0.12,
        }
    }
}

pub fn stacking(config: StackingConfig) -> impl Bundle {
    let max = config.max_stacks;
    let speed_per = config.speed_per_stack;

    // Expression: AttackSpeed = 1.0 + (Stacks * speed_per_stack)
    let attack_speed_expr = Expr::Add(
        Box::new(val(1.0)),
        Box::new(Expr::Multiply(
            Box::new(stat(stacks_stat())),
            Box::new(val(speed_per)),
        )),
    );

    (
        OnHitRules(vec![
            // Add stack and reset decay timer
            Rule::new()
                .then(Effect::SetStat {
                    stat: stacks_stat(),
                    value: Expr::Add(Box::new(stat(stacks_stat())), Box::new(val(config.gain_per_hit))),
                })
                .then(Effect::SetStat {
                    stat: decay_stat(),
                    value: val(config.decay_interval),
                }),
            // Cap at max stacks
            Rule::new()
                .when(Condition::GreaterThan(stat(stacks_stat()), val(max)))
                .then(Effect::SetStat {
                    stat: stacks_stat(),
                    value: val(max),
                }),
            // Update AttackSpeed from stacks
            Rule::new().then(Effect::SetStat {
                stat: Stat::AttackSpeed,
                value: attack_speed_expr.clone(),
            }),
        ]),
        OnCritHitRules(vec![
            // Bonus stacks on crit
            Rule::new().then(Effect::SetStat {
                stat: stacks_stat(),
                value: Expr::Add(Box::new(stat(stacks_stat())), Box::new(val(config.crit_bonus))),
            }),
            // Cap at max stacks
            Rule::new()
                .when(Condition::GreaterThan(stat(stacks_stat()), val(max)))
                .then(Effect::SetStat {
                    stat: stacks_stat(),
                    value: val(max),
                }),
            // Update AttackSpeed
            Rule::new().then(Effect::SetStat {
                stat: Stat::AttackSpeed,
                value: attack_speed_expr,
            }),
        ]),
        OnTickRules(vec![
            // Decrement decay timer
            Rule::new()
                .when(Condition::GreaterThan(stat(decay_stat()), val(0.0)))
                .then(Effect::SetStat {
                    stat: decay_stat(),
                    value: Expr::Subtract(
                        Box::new(stat(decay_stat())),
                        Box::new(action(ActionVar::DeltaTime)),
                    ),
                }),
            // Reset stacks and attack speed when timer expires
            Rule::new()
                .when(Condition::LessOrEqual(stat(decay_stat()), val(0.0)))
                .when(Condition::GreaterThan(stat(stacks_stat()), val(0.0)))
                .then(Effect::SetStat {
                    stat: stacks_stat(),
                    value: val(0.0),
                })
                .then(Effect::SetStat {
                    stat: Stat::AttackSpeed,
                    value: val(1.0),
                }),
        ]),
    )
}

//! Stacking attack speed buff rules (Jax-style).

use crate::rules::{ActionVar, Condition, Effect, Expr, Rule, Stat, action, stat, val};

fn stacks_stat() -> Stat {
    Stat::Custom("Stacks".into())
}

fn decay_stat() -> Stat {
    Stat::Custom("StackDecay".into())
}

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

pub struct StackingRules {
    pub on_hit: Vec<Rule>,
    pub on_crit_hit: Vec<Rule>,
    pub on_tick: Vec<Rule>,
}

pub fn stacking_rules(config: StackingConfig) -> StackingRules {
    let max = config.max_stacks;
    let speed_per = config.speed_per_stack;

    let attack_speed_expr = Expr::Add(
        Box::new(val(1.0)),
        Box::new(Expr::Multiply(
            Box::new(stat(stacks_stat())),
            Box::new(val(speed_per)),
        )),
    );

    StackingRules {
        on_hit: vec![
            Rule::new()
                .then(Effect::SetStat {
                    stat: stacks_stat(),
                    value: Expr::Add(
                        Box::new(stat(stacks_stat())),
                        Box::new(val(config.gain_per_hit)),
                    ),
                })
                .then(Effect::SetStat {
                    stat: decay_stat(),
                    value: val(config.decay_interval),
                }),
            Rule::new()
                .when(Condition::GreaterThan(stat(stacks_stat()), val(max)))
                .then(Effect::SetStat {
                    stat: stacks_stat(),
                    value: val(max),
                }),
            Rule::new().then(Effect::SetStat {
                stat: Stat::AttackSpeed,
                value: attack_speed_expr.clone(),
            }),
        ],
        on_crit_hit: vec![
            Rule::new().then(Effect::SetStat {
                stat: stacks_stat(),
                value: Expr::Add(
                    Box::new(stat(stacks_stat())),
                    Box::new(val(config.crit_bonus)),
                ),
            }),
            Rule::new()
                .when(Condition::GreaterThan(stat(stacks_stat()), val(max)))
                .then(Effect::SetStat {
                    stat: stacks_stat(),
                    value: val(max),
                }),
            Rule::new().then(Effect::SetStat {
                stat: Stat::AttackSpeed,
                value: attack_speed_expr,
            }),
        ],
        on_tick: vec![
            Rule::new()
                .when(Condition::GreaterThan(stat(decay_stat()), val(0.0)))
                .then(Effect::SetStat {
                    stat: decay_stat(),
                    value: Expr::Subtract(
                        Box::new(stat(decay_stat())),
                        Box::new(action(ActionVar::DeltaTime)),
                    ),
                }),
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
        ],
    }
}

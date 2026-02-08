//! Data-driven Rules System
//!
//! A compositional system where everything builds from small blocks into larger blocks.
//!
//! ## Boolean Convention
//!
//! All values are stored as `f32`. Boolean-like values use:
//! - `0.0` = false
//! - `1.0` = true
//!
//! Checks use `> 0.5` threshold to handle floating-point representation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// LEVEL 2: STORAGE - Stats (persistent) and Action (per-action)
// ============================================================================

/// Persistent entity stats - "who you are"
///
/// These values live with the entity and persist across actions.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Stats(pub HashMap<Stat, f32>);

impl Stats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, stat: Stat, value: f32) -> Self {
        self.0.insert(stat, value);
        self
    }

    pub fn get(&self, stat: &Stat) -> f32 {
        self.0.get(stat).copied().unwrap_or(0.0)
    }

    pub fn set(&mut self, stat: Stat, value: f32) {
        self.0.insert(stat, value);
    }
}

/// Per-action context - "what's happening now"
///
/// Temporary values for the current action. Created fresh, modified by rules, then consumed.
#[derive(Default, Clone, Debug)]
pub struct Action(pub HashMap<ActionVar, f32>);

impl Action {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, var: ActionVar, value: f32) -> Self {
        self.0.insert(var, value);
        self
    }

    pub fn get(&self, var: &ActionVar) -> f32 {
        self.0.get(var).copied().unwrap_or(0.0)
    }

    pub fn set(&mut self, var: ActionVar, value: f32) {
        self.0.insert(var, value);
    }
}

// ============================================================================
// STAT ENUM - Persistent entity stats
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stat {
    // === Core (engine reads/writes) ===
    Health,
    MaxHealth,

    // === Offensive ===
    AttackDamage,
    AbilityPower,

    // === Defensive ===
    Armor,
    MagicResist,

    // === Multipliers (engine reads) ===
    AttackSpeed,
    MovementSpeed,

    // === Combat ===
    CritChance,
    CritMultiplier,

    // === Attack State (synced from AttackState component) ===
    IsAttacking,
    AttackProgress,
    ComboCount,
    InWindup,
    InRecovery,

    // === Attack Parameters ===
    Knockback,
    AttackRange,
    AttackArc,

    // === Extension ===
    Custom(String),
}

// ============================================================================
// ACTIONVAR ENUM - Per-action context variables
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionVar {
    // === Damage ===
    Damage,
    DamageType,

    // === Force ===
    Knockback,
    Launch,
    Push,

    // === Context ===
    Range,
    DeltaTime,

    // === Feedback (per-action juice) ===
    HitStopDuration,
    ShakeIntensity,
    RumbleIntensity,
    RumbleDuration,
    FlashDuration,

    // === Extension ===
    Custom(String),
}

// ============================================================================
// LEVEL 1: EXPRESSIONS
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Expr {
    Value(f32),
    Stat(Stat),
    Action(ActionVar),

    Add(Box<Expr>, Box<Expr>),
    Subtract(Box<Expr>, Box<Expr>),
    Multiply(Box<Expr>, Box<Expr>),
    Divide(Box<Expr>, Box<Expr>),
    Negate(Box<Expr>),

    Min(Box<Expr>, Box<Expr>),
    Max(Box<Expr>, Box<Expr>),
    Abs(Box<Expr>),
    Floor(Box<Expr>),
    Ceil(Box<Expr>),
}

impl Expr {
    pub fn eval(&self, stats: &Stats, action: &Action) -> f32 {
        match self {
            Expr::Value(v) => *v,
            Expr::Stat(stat) => stats.get(stat),
            Expr::Action(var) => action.get(var),

            Expr::Add(a, b) => a.eval(stats, action) + b.eval(stats, action),
            Expr::Subtract(a, b) => a.eval(stats, action) - b.eval(stats, action),
            Expr::Multiply(a, b) => a.eval(stats, action) * b.eval(stats, action),
            Expr::Divide(a, b) => {
                let divisor = b.eval(stats, action);
                if divisor.abs() < f32::EPSILON {
                    0.0
                } else {
                    a.eval(stats, action) / divisor
                }
            }
            Expr::Negate(e) => -e.eval(stats, action),

            Expr::Min(a, b) => a.eval(stats, action).min(b.eval(stats, action)),
            Expr::Max(a, b) => a.eval(stats, action).max(b.eval(stats, action)),
            Expr::Abs(e) => e.eval(stats, action).abs(),
            Expr::Floor(e) => e.eval(stats, action).floor(),
            Expr::Ceil(e) => e.eval(stats, action).ceil(),
        }
    }
}

// ============================================================================
// LEVEL 3: CONDITIONS
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Condition {
    GreaterThan(Expr, Expr),
    GreaterOrEqual(Expr, Expr),
    LessThan(Expr, Expr),
    LessOrEqual(Expr, Expr),
    Equals(Expr, Expr),

    /// True if rng_roll < expr. Caller provides the roll value.
    Chance(Expr),

    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

/// Check a condition using a provided RNG roll for Chance nodes.
pub fn check_condition_with_roll(
    cond: &Condition,
    stats: &Stats,
    action: &Action,
    rng_roll: f32,
) -> bool {
    match cond {
        Condition::GreaterThan(a, b) => a.eval(stats, action) > b.eval(stats, action),
        Condition::GreaterOrEqual(a, b) => a.eval(stats, action) >= b.eval(stats, action),
        Condition::LessThan(a, b) => a.eval(stats, action) < b.eval(stats, action),
        Condition::LessOrEqual(a, b) => a.eval(stats, action) <= b.eval(stats, action),
        Condition::Equals(a, b) => {
            (a.eval(stats, action) - b.eval(stats, action)).abs() < f32::EPSILON
        }

        Condition::Chance(expr) => rng_roll < expr.eval(stats, action),

        Condition::All(conds) => conds
            .iter()
            .all(|c| check_condition_with_roll(c, stats, action, rng_roll)),
        Condition::Any(conds) => conds
            .iter()
            .any(|c| check_condition_with_roll(c, stats, action, rng_roll)),
        Condition::Not(c) => !check_condition_with_roll(c, stats, action, rng_roll),
    }
}

/// Check a condition. Chance nodes use `rng_roll = 0.5` (50/50 default).
/// Prefer `check_condition_with_roll` for deterministic behavior.
pub fn check_condition(cond: &Condition, stats: &Stats, action: &Action) -> bool {
    check_condition_with_roll(cond, stats, action, 0.5)
}

pub fn check_conditions(conditions: &[Condition], stats: &Stats, action: &Action) -> bool {
    conditions.iter().all(|c| check_condition(c, stats, action))
}

pub fn check_conditions_with_roll(
    conditions: &[Condition],
    stats: &Stats,
    action: &Action,
    rng_roll: f32,
) -> bool {
    conditions
        .iter()
        .all(|c| check_condition_with_roll(c, stats, action, rng_roll))
}

// ============================================================================
// LEVEL 4: EFFECTS
// ============================================================================

/// Events that rules can trigger (collected during execution, fired by caller)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuleEvent {
    Crit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Effect {
    SetStat { stat: Stat, value: Expr },
    SetAction { var: ActionVar, value: Expr },
    Trigger(RuleEvent),
    Log(String),
}

/// Output from rule execution - collected events and log messages.
#[derive(Default, Debug)]
pub struct RuleOutput {
    pub events: Vec<RuleEvent>,
    pub logs: Vec<String>,
}

impl RuleOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has(&self, event: &RuleEvent) -> bool {
        self.events.contains(event)
    }

    pub fn is_crit(&self) -> bool {
        self.has(&RuleEvent::Crit)
    }
}

pub fn execute_effect(
    effect: &Effect,
    stats: &mut Stats,
    action: &mut Action,
    output: &mut RuleOutput,
) {
    match effect {
        Effect::SetStat { stat, value } => {
            let v = value.eval(stats, action);
            stats.set(stat.clone(), v);
        }
        Effect::SetAction { var, value } => {
            let v = value.eval(stats, action);
            action.set(var.clone(), v);
        }
        Effect::Trigger(event) => {
            if !output.events.contains(event) {
                output.events.push(event.clone());
            }
        }
        Effect::Log(msg) => {
            output.logs.push(msg.clone());
        }
    }
}

pub fn execute_effects(effects: &[Effect], stats: &mut Stats, action: &mut Action) -> RuleOutput {
    let mut output = RuleOutput::new();
    for effect in effects {
        execute_effect(effect, stats, action, &mut output);
    }
    output
}

// ============================================================================
// LEVEL 5: RULES
// ============================================================================

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Rule {
    #[serde(default)]
    pub conditions: Vec<Condition>,
    pub effects: Vec<Effect>,
}

impl Rule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn when(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn then(mut self, effect: Effect) -> Self {
        self.effects.push(effect);
        self
    }
}

pub fn execute_rule(rule: &Rule, stats: &mut Stats, action: &mut Action, output: &mut RuleOutput) {
    if check_conditions(&rule.conditions, stats, action) {
        for effect in &rule.effects {
            execute_effect(effect, stats, action, output);
        }
    }
}

pub fn execute_rules(rules: &[Rule], stats: &mut Stats, action: &mut Action) -> RuleOutput {
    let mut output = RuleOutput::new();
    for rule in rules {
        execute_rule(rule, stats, action, &mut output);
    }
    output
}

/// Execute rules with a provided RNG roll for Chance conditions.
pub fn execute_rule_with_roll(
    rule: &Rule,
    stats: &mut Stats,
    action: &mut Action,
    output: &mut RuleOutput,
    rng_roll: f32,
) {
    if check_conditions_with_roll(&rule.conditions, stats, action, rng_roll) {
        for effect in &rule.effects {
            execute_effect(effect, stats, action, output);
        }
    }
}

pub fn execute_rules_with_roll(
    rules: &[Rule],
    stats: &mut Stats,
    action: &mut Action,
    rng_roll: f32,
) -> RuleOutput {
    let mut output = RuleOutput::new();
    for rule in rules {
        execute_rule_with_roll(rule, stats, action, &mut output, rng_roll);
    }
    output
}

// ============================================================================
// HELPER CONSTRUCTORS
// ============================================================================

pub fn val(v: f32) -> Expr {
    Expr::Value(v)
}

pub fn stat(s: Stat) -> Expr {
    Expr::Stat(s)
}

pub fn action(v: ActionVar) -> Expr {
    Expr::Action(v)
}

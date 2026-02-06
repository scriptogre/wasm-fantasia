//! Data-driven Rules System
//!
//! A compositional system where everything builds from small blocks into larger blocks.
//! See docs/implementation/RULES_SYSTEM.md for full documentation.
//!
//! ## Boolean Convention
//!
//! All values are stored as `f32`. Boolean-like values use:
//! - `0.0` = false
//! - `1.0` = true
//!
//! Checks use `> 0.5` threshold to handle floating-point representation.
//! Example: `IsCrit` is set to `1.0` for crits, and triggers check `Action(IsCrit) > 0.5`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod preset;
mod triggers;

pub use preset::*;
pub use triggers::*;

// ============================================================================
// LEVEL 2: STORAGE - Stats (persistent) and Action (per-action)
// ============================================================================

/// Persistent entity stats - "who you are"
///
/// These values live with the entity and persist across actions.
/// Examples: Health, AttackDamage, Armor, CritChance
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
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
/// Examples: Damage, Knockback, IsCrit
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
    /// Current health. Damage system reads/writes.
    Health,
    /// Maximum health.
    MaxHealth,

    // === Offensive ===
    /// Attack damage (AD) - physical scaling stat.
    AttackDamage,
    /// Ability power (AP) - magic scaling stat.
    AbilityPower,

    // === Defensive ===
    /// Physical damage reduction.
    Armor,
    /// Magic damage reduction.
    MagicResist,

    // === Multipliers (engine reads) ===
    /// Animation speed multiplier. Default: 1.0
    AttackSpeed,
    /// Movement speed multiplier. Default: 1.0
    MovementSpeed,

    // === Combat ===
    /// Critical hit chance (0.0 to 1.0).
    CritChance,
    /// Critical hit damage multiplier.
    CritMultiplier,

    // === Attack State (synced from AttackState component) ===
    /// Whether entity is currently attacking (1.0 = true, 0.0 = false).
    IsAttacking,
    /// Progress through current attack (0.0 to 1.0).
    AttackProgress,
    /// Number of attacks in current combo.
    ComboCount,
    /// Whether in windup phase before hit connects (1.0 = true).
    InWindup,
    /// Whether in recovery phase after hit connects (1.0 = true).
    InRecovery,

    // === Attack Parameters ===
    /// Base knockback force applied on hit.
    Knockback,
    /// Attack range (hitbox radius).
    AttackRange,
    /// Attack arc in degrees.
    AttackArc,

    // === Extension ===
    /// Custom stat for systems not covered above.
    Custom(String),
}

// ============================================================================
// ACTIONVAR ENUM - Per-action context variables
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionVar {
    // === Damage ===
    /// Computed damage for this action.
    Damage,
    /// Damage type: 0=physical, 1=magic, 2=true (convention).
    DamageType,

    // === Force ===
    /// Radial force away from source.
    Knockback,
    /// Vertical force (positive = up).
    Launch,
    /// Forward force in source's facing direction.
    Push,

    // === Context ===
    /// Attack/ability range.
    Range,
    /// Frame delta time (for tick rules).
    DeltaTime,

    // === Feedback (per-action juice) ===
    /// Hit stop (freeze frame) duration in seconds.
    HitStopDuration,
    /// Screen shake intensity (0.0 to 1.0).
    ShakeIntensity,
    /// Haptic rumble intensity (0.0 to 1.0, mapped to gamepad motors).
    RumbleIntensity,
    /// Haptic rumble duration in milliseconds.
    RumbleDuration,
    /// Hit flash duration on target in seconds.
    FlashDuration,

    // === Extension ===
    /// Custom action variable.
    Custom(String),
}

// ============================================================================
// LEVEL 1: EXPRESSIONS
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Expr {
    // === Leaf Values ===
    /// Literal number: `Value(5.0)`
    Value(f32),
    /// Read from persistent Stats: `Stat(Health)`
    Stat(Stat),
    /// Read from Action context: `Action(Damage)`
    Action(ActionVar),

    // === Arithmetic ===
    /// a + b
    Add(Box<Expr>, Box<Expr>),
    /// a - b
    Subtract(Box<Expr>, Box<Expr>),
    /// a * b
    Multiply(Box<Expr>, Box<Expr>),
    /// a / b
    Divide(Box<Expr>, Box<Expr>),
    /// -a
    Negate(Box<Expr>),

    // === Functions ===
    /// min(a, b)
    Min(Box<Expr>, Box<Expr>),
    /// max(a, b)
    Max(Box<Expr>, Box<Expr>),
    /// |a|
    Abs(Box<Expr>),
    /// floor(a)
    Floor(Box<Expr>),
    /// ceil(a)
    Ceil(Box<Expr>),
}

impl Expr {
    pub fn eval(&self, stats: &Stats, action: &Action) -> f32 {
        match self {
            // Leaf values
            Expr::Value(v) => *v,
            Expr::Stat(stat) => stats.get(stat),
            Expr::Action(var) => action.get(var),

            // Arithmetic
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

            // Functions
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
    // === Comparisons ===
    GreaterThan(Expr, Expr),
    GreaterOrEqual(Expr, Expr),
    LessThan(Expr, Expr),
    LessOrEqual(Expr, Expr),
    Equals(Expr, Expr),

    // === Random ===
    /// True if random() < expr
    Chance(Expr),

    // === Logical ===
    /// All conditions must be true (AND)
    All(Vec<Condition>),
    /// Any condition must be true (OR)
    Any(Vec<Condition>),
    /// Negate condition
    Not(Box<Condition>),
}

pub fn check_condition(cond: &Condition, stats: &Stats, action: &Action) -> bool {
    match cond {
        // Comparisons
        Condition::GreaterThan(a, b) => a.eval(stats, action) > b.eval(stats, action),
        Condition::GreaterOrEqual(a, b) => a.eval(stats, action) >= b.eval(stats, action),
        Condition::LessThan(a, b) => a.eval(stats, action) < b.eval(stats, action),
        Condition::LessOrEqual(a, b) => a.eval(stats, action) <= b.eval(stats, action),
        Condition::Equals(a, b) => {
            (a.eval(stats, action) - b.eval(stats, action)).abs() < f32::EPSILON
        }

        // Random
        Condition::Chance(expr) => {
            rand::Rng::random::<f32>(&mut rand::rng()) < expr.eval(stats, action)
        }

        // Logical
        Condition::All(conds) => conds.iter().all(|c| check_condition(c, stats, action)),
        Condition::Any(conds) => conds.iter().any(|c| check_condition(c, stats, action)),
        Condition::Not(c) => !check_condition(c, stats, action),
    }
}

pub fn check_conditions(conditions: &[Condition], stats: &Stats, action: &Action) -> bool {
    conditions
        .iter()
        .all(|c| check_condition(c, stats, action))
}

// ============================================================================
// LEVEL 4: EFFECTS
// ============================================================================

/// Events that rules can trigger (collected during execution, fired by caller)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuleEvent {
    /// This action is a critical hit
    Crit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Effect {
    // === Variable Modification ===
    /// Set a persistent stat
    SetStat { stat: Stat, value: Expr },
    /// Set an action context variable
    SetAction { var: ActionVar, value: Expr },

    // === Events ===
    /// Trigger an event (collected and fired after rule execution)
    Trigger(RuleEvent),

    // === Debug ===
    Log(String),
}

/// Output from rule execution - collected events to be fired by caller
#[derive(Default, Debug)]
pub struct RuleOutput {
    pub events: Vec<RuleEvent>,
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

pub fn execute_effect(effect: &Effect, stats: &mut Stats, action: &mut Action, output: &mut RuleOutput) {
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
            info!("[Rule] {}", msg);
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
    /// All conditions must be true (implicit AND)
    #[serde(default)]
    pub conditions: Vec<Condition>,
    /// Effects to execute if conditions pass
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

// ============================================================================
// HELPER CONSTRUCTORS (for ergonomic Rust code)
// ============================================================================

/// Helper to create Value expressions
pub fn val(v: f32) -> Expr {
    Expr::Value(v)
}

/// Helper to create Stat expressions
pub fn stat(s: Stat) -> Expr {
    Expr::Stat(s)
}

/// Helper to create Action expressions
pub fn action(v: ActionVar) -> Expr {
    Expr::Action(v)
}

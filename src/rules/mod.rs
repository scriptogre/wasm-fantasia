//! Data-driven Rules System

use bevy::prelude::*;
use std::collections::HashMap;
use std::ops::{Add, Mul, Neg, Sub};

mod triggers;

pub use triggers::*;

// ============================================================================
// VARIABLES
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Var {
    // System-provided
    DeltaTime,

    // Persistent entity state
    Stacks,
    Health,
    MaxHealth,
    CritRate,
    CritMultiplier,
    AttackPower,

    // Countdowns
    Inactivity,

    // Computed (per-hit)
    HitDamage,
    HitForceRadial,
    HitForceForward,
    HitForceVertical,

    // Flags (booleans stored as 0.0 or 1.0)
    IsCrit,
}

// ============================================================================
// EXPRESSIONS
// ============================================================================

#[derive(Clone, Debug)]
pub enum Expr {
    Lit(f32),
    Var(Var),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
}

impl Expr {
    pub fn eval(&self, vars: &RuleVars) -> f32 {
        match self {
            Expr::Lit(v) => *v,
            Expr::Var(var) => vars.get(*var),
            Expr::Add(a, b) => a.eval(vars) + b.eval(vars),
            Expr::Sub(a, b) => a.eval(vars) - b.eval(vars),
            Expr::Mul(a, b) => a.eval(vars) * b.eval(vars),
            Expr::Neg(e) => -e.eval(vars),
        }
    }
}

impl From<Var> for Expr {
    fn from(var: Var) -> Self {
        Expr::Var(var)
    }
}

impl From<f32> for Expr {
    fn from(val: f32) -> Self {
        Expr::Lit(val)
    }
}

// Var op Var
impl Sub for Var {
    type Output = Expr;
    fn sub(self, rhs: Var) -> Expr {
        Expr::Sub(Box::new(Expr::Var(self)), Box::new(Expr::Var(rhs)))
    }
}

impl Add for Var {
    type Output = Expr;
    fn add(self, rhs: Var) -> Expr {
        Expr::Add(Box::new(Expr::Var(self)), Box::new(Expr::Var(rhs)))
    }
}

impl Mul for Var {
    type Output = Expr;
    fn mul(self, rhs: Var) -> Expr {
        Expr::Mul(Box::new(Expr::Var(self)), Box::new(Expr::Var(rhs)))
    }
}

// Var op f32
impl Add<f32> for Var {
    type Output = Expr;
    fn add(self, rhs: f32) -> Expr {
        Expr::Add(Box::new(Expr::Var(self)), Box::new(Expr::Lit(rhs)))
    }
}

impl Sub<f32> for Var {
    type Output = Expr;
    fn sub(self, rhs: f32) -> Expr {
        Expr::Sub(Box::new(Expr::Var(self)), Box::new(Expr::Lit(rhs)))
    }
}

impl Mul<f32> for Var {
    type Output = Expr;
    fn mul(self, rhs: f32) -> Expr {
        Expr::Mul(Box::new(Expr::Var(self)), Box::new(Expr::Lit(rhs)))
    }
}

impl Neg for Var {
    type Output = Expr;
    fn neg(self) -> Expr {
        Expr::Neg(Box::new(Expr::Var(self)))
    }
}

// Expr op f32
impl Add<f32> for Expr {
    type Output = Expr;
    fn add(self, rhs: f32) -> Expr {
        Expr::Add(Box::new(self), Box::new(Expr::Lit(rhs)))
    }
}

impl Sub<f32> for Expr {
    type Output = Expr;
    fn sub(self, rhs: f32) -> Expr {
        Expr::Sub(Box::new(self), Box::new(Expr::Lit(rhs)))
    }
}

impl Mul<f32> for Expr {
    type Output = Expr;
    fn mul(self, rhs: f32) -> Expr {
        Expr::Mul(Box::new(self), Box::new(Expr::Lit(rhs)))
    }
}

impl Neg for Expr {
    type Output = Expr;
    fn neg(self) -> Expr {
        Expr::Neg(Box::new(self))
    }
}

// ============================================================================
// VAR HELPERS
// ============================================================================

impl Var {
    /// Set this var: `Stacks.set(5.0)` or `Stacks.set(Stacks + 1.0)`
    pub fn set(self, expr: impl Into<Expr>) -> Effect {
        Effect::Assign {
            var: self,
            expr: expr.into(),
        }
    }

    // === Flag helpers (for vars used as booleans) ===

    /// Set to true (1.0): `IsCrit.set_true()`
    pub fn set_true(self) -> Effect {
        self.set(1.0)
    }

    /// Set to false (0.0): `IsCrit.set_false()`
    pub fn set_false(self) -> Effect {
        self.set(0.0)
    }

    /// Check if true (> 0.5): `IsCrit.is_true()`
    pub fn is_true(self) -> Condition {
        Condition::Gt(self, 0.5)
    }

    /// Check if false (<= 0.5): `IsCrit.is_false()`
    pub fn is_false(self) -> Condition {
        Condition::Lte(self, 0.5)
    }

    // === Random ===

    /// Roll a random check using this var's value as probability
    pub fn roll(self) -> Condition {
        Condition::Roll(self)
    }

    // === Comparison helpers ===

    pub fn greater_than(self, value: f32) -> Condition {
        Condition::Gt(self, value)
    }

    pub fn greater_than_or_equal(self, value: f32) -> Condition {
        Condition::Gte(self, value)
    }

    pub fn less_than(self, value: f32) -> Condition {
        Condition::Lt(self, value)
    }

    pub fn less_than_or_equal(self, value: f32) -> Condition {
        Condition::Lte(self, value)
    }

    pub fn equals(self, value: f32) -> Condition {
        Condition::Eq(self, value)
    }
}

// ============================================================================
// STORAGE
// ============================================================================

#[derive(Component, Default, Clone, Debug)]
pub struct RuleVars(pub HashMap<Var, f32>);

impl RuleVars {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, var: Var, value: f32) -> Self {
        self.0.insert(var, value);
        self
    }

    pub fn get(&self, var: Var) -> f32 {
        self.0.get(&var).copied().unwrap_or(0.0)
    }

    pub fn set(&mut self, var: Var, value: f32) {
        self.0.insert(var, value);
    }
}

// ============================================================================
// RULE DEFINITION
// ============================================================================

#[derive(Clone, Debug, Default)]
pub struct Rule {
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

// ============================================================================
// CONDITIONS
// ============================================================================

#[derive(Clone, Debug)]
pub enum Condition {
    Gte(Var, f32),
    Gt(Var, f32),
    Lt(Var, f32),
    Lte(Var, f32),
    Eq(Var, f32),
    Chance(f32),
    Roll(Var),
}

// ============================================================================
// EFFECTS
// ============================================================================

#[derive(Clone, Debug)]
pub enum Effect {
    Assign { var: Var, expr: Expr },
    Log(String),
}

// ============================================================================
// RULE EXECUTION
// ============================================================================

pub fn check_conditions(conditions: &[Condition], vars: &RuleVars) -> bool {
    conditions.iter().all(|cond| match cond {
        Condition::Gte(var, value) => vars.get(*var) >= *value,
        Condition::Gt(var, value) => vars.get(*var) > *value,
        Condition::Lt(var, value) => vars.get(*var) < *value,
        Condition::Lte(var, value) => vars.get(*var) <= *value,
        Condition::Eq(var, value) => (vars.get(*var) - *value).abs() < f32::EPSILON,
        Condition::Chance(prob) => rand::Rng::random::<f32>(&mut rand::rng()) < *prob,
        Condition::Roll(var) => rand::Rng::random::<f32>(&mut rand::rng()) < vars.get(*var),
    })
}

pub fn execute_effects(effects: &[Effect], vars: &mut RuleVars) {
    for effect in effects {
        match effect {
            Effect::Assign { var, expr } => {
                let value = expr.eval(vars);
                vars.set(*var, value);
            }
            Effect::Log(msg) => {
                info!("[Rule] {}", msg);
            }
        }
    }
}

pub fn execute_rules(rules: &[Rule], vars: &mut RuleVars) {
    for rule in rules {
        if check_conditions(&rule.conditions, vars) {
            execute_effects(&rule.effects, vars);
        }
    }
}

//! Data-driven Rules System
//!
//! Re-exports the shared rules engine and wraps Stats as a Bevy Component.

pub use wasm_fantasia_shared::rules::{
    Action, ActionVar, Condition, Effect, Expr, Rule, RuleEvent, RuleOutput, Stat, action,
    check_condition, check_condition_with_roll, check_conditions, check_conditions_with_roll,
    execute_effect, execute_effects, execute_rule, execute_rule_with_roll, execute_rules,
    execute_rules_with_roll, stat, val,
};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

mod preset;
mod triggers;

pub use preset::*;
pub use triggers::*;

/// Bevy Component wrapper around shared Stats.
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Stats(pub wasm_fantasia_shared::rules::Stats);

impl Stats {
    pub fn new() -> Self {
        Self(wasm_fantasia_shared::rules::Stats::new())
    }

    pub fn with(mut self, stat: Stat, value: f32) -> Self {
        self.0 = self.0.with(stat, value);
        self
    }
}

impl Deref for Stats {
    type Target = wasm_fantasia_shared::rules::Stats;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Stats {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

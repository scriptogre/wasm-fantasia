//! Data-driven Rules System
//!
//! Re-exports the shared rules engine and wraps Stats as a Bevy Component.

pub use game_core::rules::{
    Action, ActionVar, Condition, Effect, Expr, Rule, RuleEvent, RuleOutput, action,
    check_condition, check_condition_with_roll, check_conditions, check_conditions_with_roll,
    execute_effect, execute_effects, execute_rule, execute_rule_with_roll, execute_rules,
    execute_rules_with_roll, stat, val,
};

use bevy::prelude::*;

// Re-export Stat and Stats from the models crate (canonical home)
pub use crate::models::combat::{Stat, Stats};

mod preset;
mod triggers;

pub use preset::*;
pub use triggers::*;

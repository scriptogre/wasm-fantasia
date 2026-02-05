//! Rule Presets - Reusable rule compositions
//!
//! Presets are factory functions that return bundles of rule components
//! for common gameplay patterns. They build on the rules system.
//!
//! # Example
//! ```rust
//! commands.spawn((
//!     Player,
//!     rule_presets::crit(CritConfig::default()),
//!     rule_presets::stacking(StackingConfig::default()),
//! ));
//! ```

mod crit;
mod stacking;

pub use crit::{CritConfig, crit};
pub use stacking::{StackingConfig, stacking};

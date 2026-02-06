//! Feedback presets - bundles of juice effects
//!
//! Level 2 abstractions that set multiple ActionVars for common feedback patterns.
//! Use these in OnPreHitRules to define how attacks "feel".
//!
//! # Example
//! ```rust
//! Rule::new()
//!     .then_all(feedback::heavy())
//!     .then(Effect::SetAction { var: Damage, value: val(100.0) })
//! ```

use crate::rules::{action, val, ActionVar, Effect, Expr};

/// Helper to create SetAction effect
fn set(var: ActionVar, value: f32) -> Effect {
    Effect::SetAction {
        var,
        value: val(value),
    }
}

/// Helper to multiply an ActionVar by a constant
fn multiply(var: ActionVar, mult: f32) -> Effect {
    Effect::SetAction {
        var: var.clone(),
        value: Expr::Multiply(Box::new(action(var)), Box::new(val(mult))),
    }
}

// ============================================================================
// FEEDBACK PRESETS
// ============================================================================

/// No feedback - silent hit (for DoT ticks, etc.)
pub fn silent() -> Vec<Effect> {
    vec![
        set(ActionVar::HitStopDuration, 0.0),
        set(ActionVar::ShakeIntensity, 0.0),
        set(ActionVar::RumbleIntensity, 0.0),
        set(ActionVar::RumbleDuration, 0.0),
        set(ActionVar::FlashDuration, 0.0),
    ]
}

/// Minimal feedback - subtle confirmation
pub fn subtle() -> Vec<Effect> {
    vec![
        set(ActionVar::HitStopDuration, 0.015),
        set(ActionVar::ShakeIntensity, 0.05),
        set(ActionVar::RumbleIntensity, 0.1),
        set(ActionVar::RumbleDuration, 30.0),
        set(ActionVar::FlashDuration, 0.04),
    ]
}

/// Light feedback - quick jabs, rapid attacks
pub fn light() -> Vec<Effect> {
    vec![
        set(ActionVar::HitStopDuration, 0.03),
        set(ActionVar::ShakeIntensity, 0.15),
        set(ActionVar::RumbleIntensity, 0.25),
        set(ActionVar::RumbleDuration, 50.0),
        set(ActionVar::FlashDuration, 0.06),
    ]
}

/// Standard feedback - normal melee hits
pub fn standard() -> Vec<Effect> {
    vec![
        set(ActionVar::HitStopDuration, 0.04),
        set(ActionVar::ShakeIntensity, 0.25),
        set(ActionVar::RumbleIntensity, 0.35),
        set(ActionVar::RumbleDuration, 60.0),
        set(ActionVar::FlashDuration, 0.08),
    ]
}

/// Punchy feedback - satisfying impact
pub fn punchy() -> Vec<Effect> {
    vec![
        set(ActionVar::HitStopDuration, 0.05),
        set(ActionVar::ShakeIntensity, 0.4),
        set(ActionVar::RumbleIntensity, 0.5),
        set(ActionVar::RumbleDuration, 80.0),
        set(ActionVar::FlashDuration, 0.1),
    ]
}

/// Heavy feedback - big slams, charged attacks
pub fn heavy() -> Vec<Effect> {
    vec![
        set(ActionVar::HitStopDuration, 0.08),
        set(ActionVar::ShakeIntensity, 0.6),
        set(ActionVar::RumbleIntensity, 0.7),
        set(ActionVar::RumbleDuration, 120.0),
        set(ActionVar::FlashDuration, 0.12),
    ]
}

/// Massive feedback - ultimate abilities, boss hits
pub fn massive() -> Vec<Effect> {
    vec![
        set(ActionVar::HitStopDuration, 0.12),
        set(ActionVar::ShakeIntensity, 0.9),
        set(ActionVar::RumbleIntensity, 1.0),
        set(ActionVar::RumbleDuration, 200.0),
        set(ActionVar::FlashDuration, 0.15),
    ]
}

// ============================================================================
// FEEDBACK MODIFIERS
// ============================================================================

/// Amplify all feedback by a multiplier (for crits, etc.)
pub fn amplify(mult: f32) -> Vec<Effect> {
    vec![
        multiply(ActionVar::HitStopDuration, mult),
        multiply(ActionVar::ShakeIntensity, mult),
        multiply(ActionVar::RumbleIntensity, mult),
        multiply(ActionVar::RumbleDuration, mult),
        // Flash doesn't scale as much - already visible
        multiply(ActionVar::FlashDuration, 1.0 + (mult - 1.0) * 0.5),
    ]
}

/// Reduce all feedback by a multiplier (for speed builds, etc.)
pub fn dampen(factor: f32) -> Vec<Effect> {
    amplify(1.0 / factor)
}

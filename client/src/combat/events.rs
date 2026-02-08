//! Combat event definitions — the complete attack chain.
//!
//! Attack chain:  [`AttackIntent`] → [`DamageDealt`] → [`HitLanded`]
//! Death chain:   [`DamageDealt`] → [`Died`] (cross-domain — any source can kill)
//! Crit chain:    [`HitLanded`] → [`CritHit`], [`Died`] → [`CritKill`]
//!
//! Convention: intents use noun form (hasn't happened yet), mutations/feedback
//! use past tense (it happened). The tense tells you the event's role.
//!
//! [`CritHit`]: crate::rules::triggers::CritHit
//! [`CritKill`]: crate::rules::triggers::CritKill

use bevy::prelude::*;
use wasm_fantasia_shared::combat::HitFeedback;

// ── Intent ──────────────────────────────────────────────────────────

/// Intent: the attack's hit frame was reached.
/// Resolved into [`DamageDealt`] + [`HitLanded`] per target.
#[derive(Event, Clone, Debug)]
pub struct AttackIntent {
    pub attacker: Entity,
}

// ── Mutations ───────────────────────────────────────────────────────

/// Mutation: damage was dealt to a target.
/// Caused by [`AttackIntent`] resolution. Triggers [`HitLanded`] and
/// potentially [`Died`].
#[derive(Event, Debug, Clone)]
pub struct DamageDealt {
    pub source: Entity,
    pub target: Entity,
    pub damage: f32,
    pub force: Vec3,
    pub is_crit: bool,
    pub feedback: HitFeedback,
}

/// Cross-domain mutation: an entity died.
/// Triggered by [`DamageDealt`] when health reaches zero.
#[derive(Event, Debug, Clone)]
pub struct Died {
    pub killer: Entity,
    pub entity: Entity,
}

// ── Feedback ────────────────────────────────────────────────────────

/// Feedback: a hit visually landed — play VFX, sound, screen shake.
/// Triggered by the [`DamageDealt`] observer.
#[derive(Event, Debug, Clone)]
pub struct HitLanded {
    pub source: Entity,
    pub target: Entity,
    pub damage: f32,
    pub is_crit: bool,
    pub feedback: HitFeedback,
}

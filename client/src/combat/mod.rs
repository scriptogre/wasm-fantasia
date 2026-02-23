use crate::*;

mod attack;
mod damage;
pub(crate) mod enemy;
mod feedback;
mod floaters;
mod sound;
mod targeting;
mod vfx;

pub use attack::{VFX_ARC_DEGREES, VFX_RANGE};
// Re-export combat types from the models crate
pub use crate::models::combat::{
    AttackIntent, AttackPhase, AttackState, Combatant, DamageDealt, Died, Enemy, EnemyBehavior,
    GroundPoundImpact, Health, HitLanded, LandingImpact, PendingKnockback, PlayerCombatant,
};
pub use feedback::*;
pub use floaters::*;
pub use targeting::LockedTarget;

pub fn plugin(app: &mut App) {
    app.add_plugins((
        attack::plugin,
        damage::plugin,
        enemy::plugin,
        feedback::plugin,
        floaters::plugin,
        vfx::plugin,
        targeting::plugin,
        sound::plugin,
    ));
}

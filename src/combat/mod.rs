use crate::models::*;
use crate::*;

mod attack;
mod components;
mod damage;
mod enemy;
mod hit_feedback;
mod separation;
mod sound;
mod targeting;

pub use attack::{VFX_ARC_DEGREES, VFX_RANGE};
pub use components::*;
pub use hit_feedback::*;
pub use targeting::LockedTarget;

pub fn plugin(app: &mut App) {
    app.add_plugins((
        components::plugin,
        attack::plugin,
        damage::plugin,
        separation::plugin,
        enemy::plugin,
        hit_feedback::plugin,
        targeting::plugin,
        sound::plugin,
    ));
}

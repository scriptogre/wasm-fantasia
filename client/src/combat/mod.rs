use crate::models::*;
use crate::*;

mod attack;
mod components;
mod damage;
mod enemy;
pub mod events;
mod feedback;
mod floaters;
mod separation;
mod sound;
mod targeting;
mod vfx;

pub use attack::{VFX_ARC_DEGREES, VFX_RANGE};
pub use components::*;
pub use events::*;
pub use feedback::*;
pub use floaters::*;
pub use targeting::LockedTarget;

pub fn plugin(app: &mut App) {
    app.add_plugins((
        components::plugin,
        attack::plugin,
        damage::plugin,
        separation::plugin,
        enemy::plugin,
        feedback::plugin,
        floaters::plugin,
        vfx::plugin,
        targeting::plugin,
        sound::plugin,
    ));
}

use crate::models::*;
use crate::*;
use avian3d::prelude::Collider;

mod components;
mod enemy;
mod hit_feedback;
mod sound;
mod systems;
mod targeting;

pub use components::*;
pub use hit_feedback::*;
pub use targeting::LockedTarget;

pub fn plugin(app: &mut App) {
    app.add_plugins((
        components::plugin,
        enemy::plugin,
        hit_feedback::plugin,
        systems::plugin,
        targeting::plugin,
        sound::plugin,
    ));
}

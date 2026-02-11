use crate::*;

pub mod combat_debug;
#[cfg(feature = "dev")]
mod dev_tools;
mod music;

pub fn plugin(app: &mut App) {
    app.add_plugins((
        models::plugin,
        scene::plugin,
        player::plugin,
        combat::plugin,
        crate::rules::plugin,
        postfx::plugin,
        music::plugin,
        combat_debug::plugin,
        #[cfg(feature = "dev")]
        dev_tools::plugin,
        screens::plugin,
    ));
}

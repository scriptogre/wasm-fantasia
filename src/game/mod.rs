use crate::*;

#[cfg(any(feature = "dev_native", not(target_arch = "wasm32")))]
mod dev_tools;
#[cfg(not(target_arch = "wasm32"))]
mod music;

pub fn plugin(app: &mut App) {
    app.add_plugins((
        models::plugin,
        scene::plugin,
        player::plugin,
        combat::plugin,
        #[cfg(not(target_arch = "wasm32"))]
        music::plugin,
        #[cfg(any(feature = "dev_native", not(target_arch = "wasm32")))]
        dev_tools::plugin,
        screens::plugin,
    ));
}

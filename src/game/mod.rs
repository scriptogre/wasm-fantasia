use crate::*;

#[cfg(any(feature = "dev_native", not(target_arch = "wasm32")))]
mod dev_tools;
mod music;

pub fn plugin(app: &mut App) {
    app.add_plugins((
        models::plugin,
        scene::plugin,
        player::plugin,
        music::plugin,
        #[cfg(any(feature = "dev_native", not(target_arch = "wasm32")))]
        dev_tools::plugin,
        screens::plugin,
    ));
}

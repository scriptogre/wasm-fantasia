use super::*;

mod modals;
mod settings;

pub use modals::*;
pub use settings::*;

pub fn plugin(app: &mut App) {
    // app.add_plugins((keybind_editor::plugin, settings::plugin));
    app.add_plugins(settings::plugin);
}

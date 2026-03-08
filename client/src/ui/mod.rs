use crate::*;
use bevy::{
    ecs::{
        spawn::SpawnRelated,
        system::{Commands, Query},
    },
    reflect::Reflect,
    ui::{
        AlignItems, BorderRadius, Display, FlexDirection, JustifyContent, Node, PositionType,
        UiRect, Val::*,
    },
    ui_widgets::Button,
    window::Window,
};
use serde::{Deserialize, Serialize};

mod death_screen;
pub mod hud;
mod interaction;
mod modal;
mod performance;
mod prefabs;
mod props;
mod server_status;
mod widget;

// constants moved to models crate as `theme` — re-export submodules for backward compat
pub use crate::models::theme::{colors, fonts, size};
pub use modal::*;
pub use prefabs::*;
pub use props::*;
pub use widget::*;

pub fn plugin(app: &mut App) {
    app.add_plugins((
        prefabs::plugin,
        interaction::plugin,
        modal::plugin,
        hud::plugin,
        death_screen::plugin,
    ));

    app.add_plugins(server_status::plugin);

    app.add_plugins(performance::plugin);
}

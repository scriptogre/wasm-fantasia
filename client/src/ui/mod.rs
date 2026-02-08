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

mod constants;
pub mod hud;
mod interaction;
mod modal;
#[cfg(feature = "dev")]
mod performance;
mod prefabs;
mod props;
#[cfg(feature = "multiplayer")]
mod server_status;
mod widget;

pub use constants::*;
pub use modal::*;
pub use prefabs::*;
pub use props::*;
pub use widget::*;

pub fn plugin(app: &mut App) {
    app.add_plugins((prefabs::plugin, interaction::plugin, modal::plugin, hud::plugin));

    #[cfg(feature = "multiplayer")]
    app.add_plugins(server_status::plugin);

    #[cfg(feature = "dev")]
    app.add_plugins(performance::plugin);
}

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
    ui_widgets::{Button, Checkbox, RadioButton},
    window::{CursorIcon, CursorOptions, SystemCursorIcon, Window},
};
use bevy_seedling::prelude::*;

mod constants;
mod interaction;
mod perf;
mod prefabs;
mod props;
mod widget;

pub use constants::*;
pub use interaction::*;
pub use prefabs::*;
pub use props::*;
pub use widget::*;

pub fn plugin(app: &mut App) {
    app.add_plugins((prefabs::plugin, interaction::plugin));

    #[cfg(feature = "dev")]
    app.add_plugins(perf::plugin);
}

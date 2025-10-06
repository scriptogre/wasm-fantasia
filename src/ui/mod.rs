use crate::*;
use bevy::{prelude::*, ui::Val::*};
use bevy_seedling::prelude::*;

mod perf;
mod prefabs;
mod widgets;

pub use prefabs::*;
pub use widgets::*;

pub fn plugin(app: &mut App) {
    app.add_plugins((UiWidgets, prefabs::plugin));

    #[cfg(feature = "dev")]
    app.add_plugins(perf::plugin);
}

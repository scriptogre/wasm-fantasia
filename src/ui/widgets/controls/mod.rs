use super::*;
use bevy::{
    app::PreUpdate,
    ecs::{
        lifecycle::RemovedComponents,
        query::Spawned,
        spawn::{Spawn, SpawnRelated, SpawnableList},
        system::{Commands, Query},
    },
    input_focus::tab_navigation::TabIndex,
    math::Rot2,
    picking::{PickingSystems, hover::Hovered},
    reflect::{Reflect, prelude::ReflectDefault},
    ui::{
        AlignItems, BorderRadius, Checked, Display, FlexDirection, InteractionDisabled,
        JustifyContent, Node, PositionType, Pressed, UiRect, UiTransform, Val,
    },
    ui_widgets::{Button, Checkbox, RadioButton},
    window::SystemCursorIcon,
};
use std::f32::consts::PI;

mod button;
mod checkbox;
mod color_swatch;
mod radio;
mod slider;

pub use button::*;
pub use checkbox::*;
pub use color_swatch::*;
pub use radio::*;
pub use slider::*;

pub fn plugin(app: &mut App) {
    app.add_plugins((button::plugin, checkbox::plugin, radio::plugin));
}

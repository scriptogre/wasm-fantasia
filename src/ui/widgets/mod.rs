//! Most of this module is copied from feathers crate as is, as encouraged
use super::*;
use bevy::{
    app::{PluginGroup, PluginGroupBuilder},
    asset::embedded_asset,
    ecs::{spawn::SpawnWith, system::IntoObserverSystem},
    input_focus::{InputDispatchPlugin, tab_navigation::TabNavigationPlugin},
    ui_widgets::UiWidgetsPlugins,
    window::Window,
};
use std::borrow::Cow;

pub mod alpha_pattern;
pub mod constants;
pub mod controls;
pub mod cursor;
pub mod font_styles;
pub mod handle_or_path;
pub mod palette;
pub mod props;
pub mod rounded_corners;
pub mod theme;
pub mod tokens;

use crate::ui::widgets::{
    alpha_pattern::{AlphaPattern, AlphaPatternMaterial, AlphaPatternResource},
    constants::{fonts, size},
    cursor::{DefaultCursor, EntityCursor},
    font_styles::InheritableFont,
    handle_or_path::HandleOrPath,
    rounded_corners::RoundedCorners,
    theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeFontColor, ThemedText, UiTheme},
};
pub use {palette::*, props::*};

pub struct UiWidgets;

impl PluginGroup for UiWidgets {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add_group(UiWidgetsPlugins)
            .add(InputDispatchPlugin)
            .add(TabNavigationPlugin)
            .add(plugin)
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<UiTheme>();

    // Embedded font
    embedded_asset!(app, "../../../assets/fonts/FiraCode-Bold.ttf");
    embedded_asset!(app, "../../../assets/fonts/FiraCode-Medium.ttf");
    embedded_asset!(app, "../../../assets/fonts/FiraCode-Regular.ttf");
    embedded_asset!(app, "../../../assets/fonts/JetBrainsMono-Italic.ttf");

    // Embedded shader
    embedded_asset!(app, "../../../assets/shaders/alpha_pattern.wgsl");

    app.add_plugins(controls::plugin);
}

/// A root UI node that fills the window and centers its content.
pub fn ui_root(name: impl Into<Cow<'static, str>>) -> impl Bundle {
    (
        Name::new(name),
        Node {
            width: Percent(100.0),
            height: Percent(100.0),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Vh(5.0),
            ..default()
        },
        // Don't block picking events for other UI roots.
        Pickable::IGNORE,
    )
}

pub fn icon(opts: impl Into<Props>) -> impl Bundle {
    let opts = opts.into();
    (
        Label,
        Name::new("Icon"),
        opts.node.clone(),
        opts.border_radius,
        children![opts.into_image_bundle()],
        Pickable::IGNORE,
    )
}
pub fn label(opts: impl Into<Props>) -> impl Bundle {
    let opts = opts.into();
    (
        Label,
        Name::new("Label"),
        opts.node.clone(),
        opts.border_radius,
        opts.into_text_bundle(),
        Pickable::IGNORE,
    )
}

/// A simple header label. Bigger than [`label`].
pub fn header(opts: impl Into<Props>) -> impl Bundle {
    let opts = opts.into();
    (Label, Name::new("Header"), opts.into_text_bundle())
}

// A regular wide button with text and an action defined as an [`Observer`].
pub fn btn_big<E, B, M, I>(opts: impl Into<Props>, action: I) -> impl Bundle
where
    E: EntityEvent,
    B: Bundle,
    I: IntoObserverSystem<E, B, M>,
{
    let opts: Props = opts.into();
    let new_node = Node {
        min_width: Vw(30.0),
        padding: UiRect::axes(Vw(8.0), Vh(2.0)),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..opts.node.clone()
    };
    let opts = opts.node(new_node);

    btn(opts, action)
}

// A small square button with text and an action defined as an [`Observer`].
pub fn btn_small<E, B, M, I>(opts: impl Into<Props>, action: I) -> impl Bundle
where
    E: EntityEvent,
    B: Bundle,
    I: IntoObserverSystem<E, B, M>,
{
    let opts: Props = opts.into();
    let new_node = Node {
        margin: UiRect::ZERO,
        padding: UiRect::ZERO,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..opts.node.clone()
    };
    let mut opts = opts.node(new_node);
    opts.border_radius = BorderRadius::all(Px(7.0));

    btn(opts, action)
}

/// A simple button with text and an action defined as an [`Observer`]. The button's layout is provided by `button_bundle`.
/// Background color is set by [`UiInteraction`]
pub fn btn<E, B, M, I>(opts: impl Into<Props>, action: I) -> impl Bundle
where
    E: EntityEvent,
    B: Bundle,
    I: IntoObserverSystem<E, B, M>,
{
    let opts: Props = opts.into();
    let action = IntoObserverSystem::into_system(action);

    (
        Name::new("Button"),
        Node::default(),
        Children::spawn(SpawnWith(move |parent: &mut ChildSpawner| {
            let content = match &opts.inner {
                WidgetContent::Image(_) => parent.spawn(opts.clone().into_image_bundle()).id(),
                WidgetContent::Text(_) => parent.spawn(opts.clone().into_text_bundle()).id(),
            };
            parent
                .spawn((
                    Button,
                    opts.border_radius,
                    opts.border_color,
                    // opts.ui_palette,
                ))
                .insert(opts.node)
                .add_children(&[content])
                .observe(action);
        })),
    )
}

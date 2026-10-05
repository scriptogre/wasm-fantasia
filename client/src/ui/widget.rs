use super::*;
use bevy::ecs::system::IntoObserverSystem;
use std::borrow::Cow;

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

/// A centered glass card for menus. Children stack and stretch to its width.
pub fn panel(max_width: f32) -> impl Bundle {
    (
        Name::new("Panel"),
        Node {
            width: Percent(92.0),
            max_width: Px(max_width),
            flex_direction: FlexDirection::Column,
            row_gap: Px(12.0),
            // Slimmer sides leave room for label + control rows on 320px phones
            padding: UiRect::axes(Px(20.0), Px(24.0)),
            border: UiRect::all(Px(1.0)),
            border_radius: BorderRadius::all(size::BORDER_RADIUS),
            ..default()
        },
        BackgroundColor(colors::GLASS),
        BorderColor::all(colors::NEUTRAL800),
        BoxShadow::new(
            Color::BLACK.with_alpha(0.5),
            Px(0.0),
            Px(12.0),
            Px(0.0),
            Px(32.0),
        ),
    )
}

pub fn icon(opts: impl Into<Props>) -> impl Bundle {
    let opts = opts.into();
    (
        Label,
        Name::new("Icon"),
        opts.node.clone(),
        children![opts.into_image_bundle()],
        Pickable::IGNORE,
    )
}

/// Plain text. Takes layout from [`Props`] but never the button chrome.
pub fn label(opts: impl Into<Props>) -> impl Bundle {
    let opts = opts.into();
    let node = Node {
        border: UiRect::ZERO,
        padding: UiRect::ZERO,
        min_height: Auto,
        ..opts.node.clone()
    };
    (
        Label,
        Name::new("Label"),
        node,
        opts.into_text_bundle(),
        Pickable::IGNORE,
    )
}

/// Panel title. Bigger and brighter than [`label`].
pub fn header(opts: impl Into<Props>) -> impl Bundle {
    let opts: Props = opts.into();
    let opts = opts.font_size(size::HEADER_SIZE).color(colors::NEUTRAL50);
    (Label, Name::new("Header"), opts.into_text_bundle())
}

/// A button with text and an action defined as an [`Observer`]. Layout comes from [`Props`],
/// colors from its [`PaletteSet`].
pub fn btn<E, B, M, I>(opts: impl Into<Props>, action: I) -> impl Bundle
where
    E: EntityEvent,
    B: Bundle,
    I: IntoObserverSystem<E, B, M>,
{
    let mut opts: Props = opts.into();
    let action = IntoObserverSystem::into_system(action);
    // The wrapper takes the outer layout, the content fills it
    let n = &opts.node;
    let wrapper = Node {
        width: n.width,
        height: n.height,
        min_width: n.min_width,
        min_height: n.min_height,
        max_width: n.max_width,
        max_height: n.max_height,
        flex_grow: n.flex_grow,
        flex_shrink: n.flex_shrink,
        flex_basis: n.flex_basis,
        align_self: n.align_self,
        justify_self: n.justify_self,
        margin: n.margin,
        ..default()
    };

    (
        Button,
        Name::new("Button"),
        wrapper,
        Pickable::IGNORE,
        Children::spawn(SpawnWith(move |parent: &mut ChildSpawner| {
            let idle = opts.palette_set.none.clone();
            let content = match &opts.content {
                WidgetContent::Image(_) => parent
                    .spawn((opts.clone().into_image_bundle(), Pickable::IGNORE))
                    .id(),
                WidgetContent::Text(_) => parent
                    .spawn((
                        opts.clone().color(idle.text).into_text_bundle(),
                        Pickable::IGNORE,
                    ))
                    .id(),
            };
            opts.node.width = Percent(100.0);
            opts.node.height = Percent(100.0);
            opts.node.max_width = Auto;
            opts.node.max_height = Auto;
            opts.node.margin = UiRect::ZERO;

            parent
                .spawn((
                    Name::new("Button Content"),
                    BackgroundColor(idle.bg),
                    idle.border,
                    opts.palette_set,
                ))
                .insert(opts.node)
                .add_children(&[content])
                .observe(action);
        })),
    )
}

// courtesy of @jannhohenheim
pub(crate) fn plus_minus_bar<E, B, M, I1, I2>(
    label_marker: impl Component,
    lower: I1,
    raise: I2,
) -> impl Bundle
where
    E: EntityEvent,
    B: Bundle,
    I1: IntoObserverSystem<E, B, M>,
    I2: IntoObserverSystem<E, B, M>,
{
    let spinner = |text: &'static str| {
        Props::new(text)
            .width(size::BUTTON_HEIGHT)
            .min_width(size::BUTTON_HEIGHT)
            .padding(UiRect::ZERO)
    };

    (
        Node {
            align_items: AlignItems::Center,
            column_gap: Px(4.0),
            ..default()
        },
        children![
            btn(spinner("-"), lower),
            (
                label(Props::new("").color(colors::NEUTRAL50).node(Node {
                    width: Px(48.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                })),
                label_marker,
            ),
            btn(spinner("+"), raise),
        ],
    )
}

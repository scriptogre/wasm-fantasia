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

/// Non-interactive button with disabled styling. Layout is fully controlled by Props.
pub fn btn_disabled(opts: impl Into<Props>) -> impl Bundle {
    let opts: Props = opts.into();
    let disabled = PaletteSet::default().disabled;
    let text = opts.clone().color(disabled.text).into_text_bundle();
    (
        Name::new("Button (Disabled)"),
        opts.node,
        BackgroundColor(disabled.bg),
        disabled.border,
        opts.border_radius,
        Pickable::IGNORE,
        children![(text, Pickable::IGNORE)],
    )
}

/// A simple button with text and an action defined as an [`Observer`]. The button's layout is provided by `button_bundle`.
/// Background color is set by [`UiPalette`]
pub fn btn<E, B, M, I>(opts: impl Into<Props>, action: I) -> impl Bundle
where
    E: EntityEvent,
    B: Bundle,
    I: IntoObserverSystem<E, B, M>,
{
    let mut opts: Props = opts.into();
    let action = IntoObserverSystem::into_system(action);

    (
        Button,
        Name::new("Button"),
        Node::default(),
        Pickable::IGNORE,
        Children::spawn(SpawnWith(move |parent: &mut ChildSpawner| {
            let content = match &opts.content {
                WidgetContent::Image(_) => parent
                    .spawn((opts.clone().into_image_bundle(), Pickable::IGNORE))
                    .id(),
                WidgetContent::Text(_) => parent
                    .spawn((opts.clone().into_text_bundle(), Pickable::IGNORE))
                    .id(),
            };
            opts.node.width = Percent(100.0);
            opts.node.height = Percent(100.0);

            parent
                .spawn((
                    Name::new("Button Content"),
                    opts.bg_color,
                    opts.border_radius,
                    opts.border_color,
                    opts.palette_set.clone(),
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
            .font_size(14.0)
            .margin(UiRect::ZERO)
            .padding(UiRect::axes(Px(8.0), Px(2.0)))
    };

    (
        Node {
            align_items: AlignItems::Center,
            ..default()
        },
        children![
            btn(spinner("-"), lower),
            (
                label(Props::new("").node(Node {
                    width: Px(80.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                })),
                label_marker,
            ),
            btn(spinner("+"), raise),
        ],
    )
}

use super::*;
use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    input_focus::{InputFocus, InputFocusVisible, tab_navigation::TabIndex},
    picking::hover::Hovered,
};
use bevy_seedling::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, (style_buttons, style_focus, scroll_menus))
        .add_observer(
            |on: On<Pointer<Press>>,
             focusable: Query<(), With<TabIndex>>,
             mut focus: ResMut<InputFocus>,
             mut visible: ResMut<InputFocusVisible>| {
                if focusable.contains(on.entity) {
                    focus.set(on.entity);
                    visible.0 = false;
                }
            },
        )
        .add_observer(
            |on: On<Activate>,
             buttons: Query<(), (With<Button>, Without<InteractionDisabled>)>,
             sources: Option<Res<AudioSources>>,
             settings: Res<Settings>,
             mut commands: Commands| {
                if buttons.contains(on.entity)
                    && let Some(sources) = sources
                {
                    commands.spawn(
                        SamplePlayer::new(sources.press.clone()).with_volume(settings.sfx()),
                    );
                }
            },
        )
        .add_observer(
            |on: On<Pointer<Drag>>, mut scroll: Query<&mut ScrollPosition, With<MenuScroll>>| {
                if let Ok(mut scroll) = scroll.get_mut(on.entity) {
                    scroll.y -= on.delta.y;
                }
            },
        );
}

fn style_buttons(
    mut buttons: Query<(
        Entity,
        &PaletteSet,
        &Hovered,
        Has<Pressed>,
        Has<InteractionDisabled>,
        Has<super::runes::InlineTerm>,
        &Children,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut texts: Query<(&mut TextColor, Has<bevy::text::Underline>)>,
    mut images: Query<&mut ImageNode>,
    focus: Res<InputFocus>,
    visible: Res<InputFocusVisible>,
    mut commands: Commands,
) {
    for (
        entity,
        palette,
        hovered,
        pressed,
        disabled,
        inline,
        children,
        mut background,
        mut border,
    ) in &mut buttons
    {
        let focused = focus.get() == Some(entity) && visible.0;
        let state = if disabled {
            &palette.disabled
        } else if pressed {
            &palette.pressed
        } else if hovered.get() || focused {
            &palette.hovered
        } else {
            &palette.none
        };
        let color = if inline { Color::NONE } else { state.bg };
        if background.0 != color {
            background.0 = color;
        }
        if *border != state.border {
            *border = state.border;
        }
        for child in children {
            if let Ok((mut color, underlined)) = texts.get_mut(*child) {
                let desired = if inline { colors::INK } else { state.text };
                if color.0 != desired {
                    color.0 = desired;
                }
                if inline && (hovered.get() || focused) && !underlined {
                    commands.entity(*child).insert(bevy::text::Underline);
                } else if inline && !(hovered.get() || focused) && underlined {
                    commands.entity(*child).remove::<bevy::text::Underline>();
                }
            }
            if let Ok(mut image) = images.get_mut(*child) {
                let desired = if inline { colors::INK } else { state.text };
                if image.color != desired {
                    image.color = desired;
                }
            }
        }
    }
}

fn scroll_menus(
    mut wheel: MessageReader<MouseWheel>,
    mut menus: Query<&mut ScrollPosition, With<MenuScroll>>,
) {
    let delta: f32 = wheel
        .read()
        .map(|event| {
            event.y
                * if event.unit == MouseScrollUnit::Line {
                    32.0
                } else {
                    1.0
                }
        })
        .sum();
    if delta != 0.0 {
        for mut scroll in &mut menus {
            scroll.y -= delta;
        }
    }
}

fn style_focus(
    focus: Res<InputFocus>,
    visible: Res<InputFocusVisible>,
    mut outlines: Query<(Entity, &mut Outline), With<TabIndex>>,
) {
    for (entity, mut outline) in &mut outlines {
        let width = Px(if focus.get() == Some(entity) && visible.0 {
            1.0
        } else {
            0.0
        });
        if outline.width != width {
            *outline = Outline {
                width,
                offset: Px(3.0),
                color: colors::INK_SOFT,
            };
        }
    }
}

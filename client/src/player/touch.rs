use bevy::{input::InputSystems, prelude::*, window::PrimaryWindow};
use bevy_enhanced_input::prelude::*;

use crate::models::{Attack, Jump, Navigate, PlayerCtx, SceneCamera, Screen};
use crate::ui::{Modal, NewModal, colors, size};

#[derive(Resource, Default)]
pub struct TouchControls {
    pub enabled: bool,
}

#[derive(Component)]
struct TouchUi;

#[derive(Component, Clone, Copy)]
enum TouchButton {
    Move,
    Attack,
    Jump,
    Pause,
}

/// Joystick thumb, follows the move drag
#[derive(Component)]
struct TouchKnob;

/// Knob offset in px at full stick deflection
const KNOB_TRAVEL: f32 = 36.0;
/// Dark glass so controls read on any background without hiding it
const IDLE: Color = Color::oklcha(0.145, 0.0, 0.0, 0.35);

pub fn plugin(app: &mut App) {
    #[cfg(not(target_arch = "wasm32"))]
    let enabled = false;
    #[cfg(target_arch = "wasm32")]
    let enabled = web_sys::window().is_some_and(|window| window.navigator().max_touch_points() > 0);
    app.insert_resource(TouchControls { enabled })
        .add_systems(OnEnter(Screen::Gameplay), spawn_controls)
        .add_systems(
            PreUpdate,
            touch_input
                .after(InputSystems)
                .before(EnhancedInputSystems::Update),
        );
}

fn spawn_controls(mut commands: Commands) {
    let edge = size::EDGE;
    let rim = colors::NEUTRAL50.with_alpha(0.2);
    let circle = |diameter: f32| Node {
        position_type: PositionType::Absolute,
        width: px(diameter),
        height: px(diameter),
        border: UiRect::all(px(1.5)),
        border_radius: BorderRadius::MAX,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    };
    let caption = |text: &'static str| {
        (
            Text::new(text),
            TextFont::from_font_size(size::CAPTION_SIZE),
            TextColor(colors::NEUTRAL50.with_alpha(0.85)),
            Pickable::IGNORE,
        )
    };
    let pause_bar = || {
        (
            Node {
                width: px(4),
                height: px(14),
                border_radius: BorderRadius::all(px(1)),
                ..default()
            },
            BackgroundColor(colors::NEUTRAL50.with_alpha(0.85)),
            Pickable::IGNORE,
        )
    };

    commands.spawn((
        TouchUi,
        DespawnOnExit(Screen::Gameplay),
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            ..default()
        },
        Visibility::Hidden,
        GlobalZIndex(100),
        Pickable::IGNORE,
        children![
            (
                TouchButton::Move,
                Node {
                    left: px(edge),
                    bottom: px(edge),
                    ..circle(132.0)
                },
                BackgroundColor(IDLE),
                BorderColor::all(rim),
                Pickable::IGNORE,
                children![(
                    TouchKnob,
                    Node {
                        width: px(52),
                        height: px(52),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(colors::NEUTRAL50.with_alpha(0.25)),
                    Pickable::IGNORE,
                )],
            ),
            (
                TouchButton::Attack,
                Node {
                    right: px(edge),
                    bottom: px(edge + 8.0),
                    ..circle(84.0)
                },
                BackgroundColor(IDLE),
                BorderColor::all(colors::AMBER.with_alpha(0.7)),
                Pickable::IGNORE,
                children![caption("HIT")],
            ),
            (
                TouchButton::Jump,
                Node {
                    right: px(edge + 100.0),
                    bottom: px(edge + 80.0),
                    ..circle(60.0)
                },
                BackgroundColor(IDLE),
                BorderColor::all(rim),
                Pickable::IGNORE,
                children![caption("JUMP")],
            ),
            (
                TouchButton::Pause,
                Node {
                    right: px(edge),
                    top: px(edge),
                    column_gap: px(4),
                    ..circle(44.0)
                },
                BackgroundColor(IDLE),
                BorderColor::all(rim),
                Pickable::IGNORE,
                children![pause_bar(), pause_bar()],
            ),
        ],
    ));
}

fn touch_input(
    touches: Res<Touches>,
    mut controls: ResMut<TouchControls>,
    window: Single<&Window, With<PrimaryWindow>>,
    screen: Res<State<Screen>>,
    players: Query<Entity, With<PlayerCtx>>,
    mut ui: Query<&mut Visibility, With<TouchUi>>,
    mut knob: Query<&mut UiTransform, With<TouchKnob>>,
    mut buttons: Query<(
        &TouchButton,
        &ComputedNode,
        &UiGlobalTransform,
        &mut BackgroundColor,
    )>,
    mut actions: Query<(
        &mut ActionMock,
        Has<Action<Navigate>>,
        Has<Action<Attack>>,
        Has<Action<Jump>>,
    )>,
    mut camera: Query<&mut Transform, With<SceneCamera>>,
    mut commands: Commands,
) {
    controls.enabled |= touches.iter_just_pressed().next().is_some();
    let active = controls.enabled
        && *screen.get() == Screen::Gameplay
        && !players.is_empty()
        && window.focused;
    for mut visibility in &mut ui {
        *visibility = if active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if !controls.enabled {
        return;
    }

    let mut pointer_scale = Vec2::splat(window.scale_factor());
    #[cfg(target_arch = "wasm32")]
    if let Some(browser) = web_sys::window() {
        let width = browser
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(window.width() as f64);
        let height = browser
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(window.height() as f64);
        pointer_scale *= Vec2::new(
            window.physical_width() as f32 / width as f32,
            window.physical_height() as f32 / height as f32,
        ) / browser.device_pixel_ratio() as f32;
    }
    let mut movement = Vec2::ZERO;
    let mut attack = false;
    let mut jump = false;
    let mut pause = false;
    let mut look = Vec2::ZERO;
    if active {
        for touch in touches.iter() {
            let start = touch.start_position() * pointer_scale;
            let button = buttons.iter().find_map(|(button, node, transform, _)| {
                node.contains_point(*transform, start)
                    .then_some((*button, node.inverse_scale_factor()))
            });
            match button {
                Some((TouchButton::Move, scale)) => {
                    let delta =
                        (touch.position() - touch.start_position()) * pointer_scale * scale / 48.0;
                    movement = Vec2::new(delta.x, -delta.y).clamp_length_max(1.0);
                }
                Some((TouchButton::Attack, _)) => attack = true,
                Some((TouchButton::Jump, _)) => jump = true,
                Some((TouchButton::Pause, _)) => pause |= touches.just_pressed(touch.id()),
                None if start.x > window.physical_width() as f32 * 0.45 => look += touch.delta(),
                _ => {}
            }
        }
    }
    for (mut mock, navigate, hit, leap) in &mut actions {
        let (value, fired): (ActionValue, bool) = if navigate {
            (movement.into(), movement.length_squared() > 0.01)
        } else if hit {
            (attack.into(), attack)
        } else if leap {
            (jump.into(), jump)
        } else {
            continue;
        };
        *mock = ActionMock::new(
            if fired {
                ActionState::Fired
            } else {
                ActionState::None
            },
            value,
            MockSpan::Manual,
        );
    }
    if pause && let Ok(player) = players.single() {
        commands.trigger(NewModal {
            entity: player,
            modal: Modal::Main,
        });
    }
    for (button, _, _, mut color) in &mut buttons {
        let pressed = match button {
            TouchButton::Move => movement.length_squared() > 0.01,
            TouchButton::Attack => attack,
            TouchButton::Jump => jump,
            TouchButton::Pause => pause,
        };
        color.0 = if pressed {
            colors::AMBER.with_alpha(0.35)
        } else {
            IDLE
        };
    }
    let thumb = Val2::px(movement.x * KNOB_TRAVEL, -movement.y * KNOB_TRAVEL);
    for mut knob in &mut knob {
        if knob.translation != thumb {
            knob.translation = thumb;
        }
    }
    if look != Vec2::ZERO
        && let Ok(mut camera) = camera.single_mut()
    {
        let (yaw, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
        camera.rotation = Quat::from_euler(
            EulerRot::YXZ,
            yaw - look.x * 0.004,
            (pitch - look.y * 0.004).clamp(-1.25, -0.1),
            0.0,
        );
    }
}

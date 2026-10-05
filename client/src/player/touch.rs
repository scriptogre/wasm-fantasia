use bevy::{input::InputSystems, prelude::*, window::PrimaryWindow};
use bevy_enhanced_input::prelude::*;

use crate::models::{Attack, Jump, Navigate, PlayerCtx, SceneCamera, Screen};

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
}

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
    commands
        .spawn((
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
        ))
        .with_children(|parent| {
            for (button, label, size, right, bottom) in [
                (TouchButton::Move, "MOVE", 144.0, false, 24.0),
                (TouchButton::Attack, "HIT", 88.0, true, 24.0),
                (TouchButton::Jump, "JUMP", 64.0, true, 132.0),
            ] {
                parent
                    .spawn((
                        button,
                        Node {
                            position_type: PositionType::Absolute,
                            width: px(size),
                            height: px(size),
                            left: if right { Val::Auto } else { px(24) },
                            right: if right { px(24) } else { Val::Auto },
                            bottom: px(bottom),
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::MAX,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.07, 0.1, 0.15, 0.55)),
                        BorderColor::all(Color::srgba(0.8, 0.9, 1.0, 0.65)),
                        Pickable::IGNORE,
                    ))
                    .with_child((
                        Text::new(label),
                        TextFont {
                            font_size: 16.0,
                            ..default()
                        },
                        Pickable::IGNORE,
                    ));
            }
            parent.spawn((
                Text::new("Drag to look. Hold HIT to attack. Jump + HIT to slam."),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    top: px(130),
                    left: px(24),
                    right: px(24),
                    ..default()
                },
                Pickable::IGNORE,
            ));
        });
}

fn touch_input(
    touches: Res<Touches>,
    mut controls: ResMut<TouchControls>,
    window: Single<&Window, With<PrimaryWindow>>,
    screen: Res<State<Screen>>,
    players: Query<(), With<PlayerCtx>>,
    mut ui: Query<&mut Visibility, With<TouchUi>>,
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
    for (button, _, _, mut color) in &mut buttons {
        let pressed = match button {
            TouchButton::Move => movement.length_squared() > 0.01,
            TouchButton::Attack => attack,
            TouchButton::Jump => jump,
        };
        color.0 = if pressed {
            Color::srgba(0.2, 0.6, 0.85, 0.8)
        } else {
            Color::srgba(0.07, 0.1, 0.15, 0.55)
        };
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

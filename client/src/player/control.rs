use super::*;

pub const IDLE_TO_RUN_TRESHOLD: f32 = 0.01;
pub const JUMP_HEIGHT: f32 = 2.0;
const BUFFER_DURATION: f32 = 0.12;

pub use crate::combat::{GroundPoundImpact, LandingImpact};

#[derive(Event)]
pub struct JumpLaunched {
    pub height: f32,
    pub position: Vec3,
}

#[derive(Component, Default)]
pub struct AirborneTracker {
    pub was_airborne: bool,
    pub peak_downward_velocity: f32,
}

#[derive(Resource, Default)]
pub struct InputBuffer {
    pub jump: Option<f32>,
    pub attack: Option<f32>,
}

impl InputBuffer {
    pub fn buffer_attack(&mut self) {
        self.attack = Some(BUFFER_DURATION);
    }
}

#[derive(Resource, Default)]
pub struct JumpState {
    pub active: bool,
}

#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Sprinting;

#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct GroundPoundState;

#[derive(Event)]
pub struct Footstep {
    pub position: Vec3,
    pub is_sprinting: bool,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<InputBuffer>()
        .init_resource::<JumpState>()
        .add_systems(
            FixedUpdate,
            (movement, tick_ground_pound)
                .chain()
                .in_set(TnuaUserControlsSystems)
                .run_if(in_state(Screen::Gameplay)),
        )
        .add_systems(
            Update,
            (tick_input_buffer, detect_landing, process_buffered_jump)
                .chain()
                .run_if(in_state(Screen::Gameplay)),
        )
        .add_systems(OnExit(Screen::Gameplay), reset_controls)
        .add_observer(on_jump)
        .add_observer(sprint_start)
        .add_observer(sprint_end)
        .add_observer(crouch_in)
        .add_observer(crouch_out);
}

fn reset_controls(mut buffer: ResMut<InputBuffer>, mut jump: ResMut<JumpState>) {
    *buffer = InputBuffer::default();
    *jump = JumpState::default();
}

fn tick_input_buffer(time: Res<Time>, mut buffer: ResMut<InputBuffer>) {
    let dt = time.delta_secs();
    if let Some(remaining) = &mut buffer.jump {
        *remaining -= dt;
        if *remaining <= 0.0 {
            buffer.jump = None;
        }
    }
    if let Some(remaining) = &mut buffer.attack {
        *remaining -= dt;
        if *remaining <= 0.0 {
            buffer.attack = None;
        }
    }
}

fn movement(
    cfg: Res<Config>,
    navigate: Query<&Action<Navigate>>,
    crouch: Query<&Action<Crouch>>,
    camera: Query<&Transform, With<SceneCamera>>,
    jump_state: Res<JumpState>,
    mut players: Query<(
        &mut Player,
        &mut TnuaController<ControlScheme>,
        &AttackState,
        Has<GroundPoundState>,
        Has<Sprinting>,
    )>,
) {
    let input = navigate.single().map(|action| **action).unwrap_or_default();
    let crouching = crouch.single().is_ok_and(|action| **action);
    let direction = camera
        .single()
        .map(|camera| camera.movement_direction(input))
        .unwrap_or_default();

    for (mut player, mut controller, attack, pounding, sprinting) in &mut players {
        let speed_factor = if crouching {
            cfg.player.movement.crouch_factor
        } else if sprinting && !attack.is_attacking() {
            cfg.player.movement.sprint_factor
        } else {
            1.0
        };
        player.speed = cfg.player.movement.speed * speed_factor;
        let attack_factor = if attack.in_windup() { 0.55 } else { 1.0 };
        controller.initiate_action_feeding();
        controller.basis = TnuaBuiltinWalk {
            desired_motion: if pounding {
                Vec3::ZERO
            } else {
                direction * player.speed * attack_factor
            },
            desired_forward: if attack.is_attacking() {
                None
            } else {
                Dir3::new(direction).ok()
            },
        };
        if jump_state.active {
            controller.action(ControlScheme::Jump(TnuaBuiltinJump {
                allow_in_air: false,
                ..default()
            }));
        }
        if crouching && !jump_state.active {
            controller.action(ControlScheme::Crouch(TnuaBuiltinCrouch));
        }
    }
}

fn on_jump(
    on: On<Start<Jump>>,
    mut commands: Commands,
    mut buffer: ResMut<InputBuffer>,
    mut jump: ResMut<JumpState>,
    query: Query<(&TnuaController<ControlScheme>, &Transform), With<Player>>,
) {
    let Ok((controller, transform)) = query.get(on.context) else {
        return;
    };
    if controller.basis_memory.standing_on_entity().is_none() {
        buffer.jump = Some(BUFFER_DURATION);
        return;
    }
    jump.active = true;
    commands.trigger(JumpLaunched {
        height: JUMP_HEIGHT,
        position: transform.translation,
    });
}

fn process_buffered_jump(
    mut buffer: ResMut<InputBuffer>,
    mut jump: ResMut<JumpState>,
    mut commands: Commands,
    query: Query<(&Transform, &TnuaController<ControlScheme>), With<Player>>,
) {
    if buffer.jump.is_none() {
        return;
    }
    let Ok((transform, controller)) = query.single() else {
        return;
    };
    if controller.basis_memory.standing_on_entity().is_none() {
        return;
    }
    buffer.jump = None;
    jump.active = true;
    commands.trigger(JumpLaunched {
        height: JUMP_HEIGHT,
        position: transform.translation,
    });
}

fn detect_landing(
    mut commands: Commands,
    mut jump: ResMut<JumpState>,
    mut query: Query<
        (
            Entity,
            &TnuaController<ControlScheme>,
            &Transform,
            &LinearVelocity,
            &mut AirborneTracker,
            Has<GroundPoundState>,
        ),
        With<Player>,
    >,
) {
    let Ok((entity, controller, transform, velocity, mut tracker, pounding)) = query.single_mut()
    else {
        return;
    };
    if controller.basis_memory.standing_on_entity().is_none() {
        tracker.peak_downward_velocity = tracker.peak_downward_velocity.min(velocity.y);
        tracker.was_airborne = true;
    } else if tracker.was_airborne {
        if pounding {
            commands.trigger(GroundPoundImpact {
                position: transform.translation,
            });
            commands.entity(entity).remove::<GroundPoundState>();
        }
        let impact = tracker.peak_downward_velocity.abs();
        if impact > 3.0 {
            commands.trigger(LandingImpact {
                velocity_y: impact,
                position: transform.translation,
            });
        }
        *tracker = AirborneTracker::default();
        jump.active = false;
    }
}

fn tick_ground_pound(
    mut query: Query<&mut LinearVelocity, (With<Player>, With<GroundPoundState>)>,
) {
    for mut velocity in &mut query {
        velocity.0 = Vec3::new(0.0, -25.0, 0.0);
    }
}

fn sprint_start(on: On<Start<Sprint>>, mut commands: Commands) {
    commands.entity(on.context).try_insert(Sprinting);
}

fn sprint_end(on: On<Complete<Sprint>>, mut commands: Commands) {
    commands.entity(on.context).try_remove::<Sprinting>();
}

pub fn crouch_in(
    on: On<Start<Crouch>>,
    mut query: Query<(&mut TnuaAvian3dSensorShape, &mut Collider), With<Player>>,
) -> Result {
    let (mut sensor, mut collider) = query.get_mut(on.context)?;
    collider.set_scale(Vec3::new(1.0, 0.5, 1.0), 4);
    sensor.0.set_scale(Vec3::new(1.0, 0.5, 1.0), 4);
    Ok(())
}

pub fn crouch_out(
    on: On<Complete<Crouch>>,
    mut query: Query<(&mut TnuaAvian3dSensorShape, &mut Collider), With<Player>>,
) -> Result {
    let (mut sensor, mut collider) = query.get_mut(on.context)?;
    collider.set_scale(Vec3::ONE, 4);
    sensor.0.set_scale(Vec3::ONE, 4);
    Ok(())
}

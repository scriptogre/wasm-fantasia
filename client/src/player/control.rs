use super::*;
use bevy_tnua::{
    builtins::{TnuaBuiltinCrouch, TnuaBuiltinDash},
    control_helpers::TnuaSimpleAirActionsCounter,
};

pub const IDLE_TO_RUN_TRESHOLD: f32 = 0.01;

/// Apply response curve to stick input for more precise control at low values.
/// Exponent > 1.0 makes small inputs smaller (more precision), large inputs relatively larger.
fn apply_response_curve(input: Vec2, exponent: f32) -> Vec2 {
    let length = input.length();
    if length < 0.001 {
        return Vec2::ZERO;
    }
    // Preserve direction, apply curve to magnitude
    let curved_length = length.powf(exponent);
    input.normalize() * curved_length
}

/// Movement stick uses slight curve (1.3) for precise positioning
const MOVEMENT_CURVE_EXPONENT: f32 = 1.3;

fn jump_action() -> TnuaBuiltinJump {
    TnuaBuiltinJump {
        height: 5.0,
        takeoff_extra_gravity: 40.0,
        fall_extra_gravity: 35.0,
        shorten_extra_gravity: 80.0,
        peak_prevention_at_upward_velocity: 0.5,
        peak_prevention_extra_gravity: 25.0,
        reschedule_cooldown: Some(0.1),
        disable_force_forward_after_peak: true,
        allow_in_air: true,
        ..Default::default()
    }
}

// ============================================================================
// INPUT BUFFERING
// Queue inputs briefly so they execute when possible (e.g., jump on landing)
// ============================================================================

/// How long buffered inputs remain valid
const BUFFER_DURATION: f32 = 0.12; // 120ms - feels responsive without being sloppy

#[derive(Resource, Default)]
pub struct InputBuffer {
    pub jump: Option<f32>, // Time remaining
    pub dash: Option<f32>,
    pub attack: Option<f32>,
}

impl InputBuffer {
    pub fn buffer_jump(&mut self) {
        self.jump = Some(BUFFER_DURATION);
    }
    pub fn buffer_dash(&mut self) {
        self.dash = Some(BUFFER_DURATION);
    }
    pub fn buffer_attack(&mut self) {
        self.attack = Some(BUFFER_DURATION);
    }
    pub fn consume_attack(&mut self) -> bool {
        self.attack.take().is_some()
    }
    pub fn consume_dash(&mut self) -> bool {
        self.dash.take().is_some()
    }
    pub fn tick(&mut self, dt: f32) {
        for timer in [&mut self.jump, &mut self.dash, &mut self.attack] {
            if let Some(t) = timer {
                *t -= dt;
                if *t <= 0.0 {
                    *timer = None;
                }
            }
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<InputBuffer>()
        .add_systems(
            Update,
            (
                movement.in_set(TnuaUserControlsSystems),
                tick_input_buffer,
                process_buffered_jump,
                process_buffered_dash,
            )
                .run_if(in_state(Screen::Gameplay)),
        )
        .add_observer(handle_jump)
        .add_observer(handle_dash)
        .add_observer(crouch_in)
        .add_observer(crouch_out);
}

fn tick_input_buffer(time: Res<Time>, mut buffer: ResMut<InputBuffer>) {
    buffer.tick(time.delta_secs());
}

/// Tnua configuration is tricky to grasp from the get go, this is the best demo:
/// <https://github.com/idanarye/bevy-tnua/blob/main/demos/src/character_control_systems/platformer_control_systems.rs>
fn movement(
    cfg: Res<Config>,
    navigate: Single<&Action<Navigate>>,
    crouch: Single<&Action<Crouch>>,
    camera: Query<&Transform, With<SceneCamera>>,
    mut player_query: Query<(&mut Player, &mut TnuaController, &mut StepTimer)>,
) -> Result {
    let (navigate, crouch) = (*navigate.into_inner(), *crouch.into_inner());

    for (player, mut controller, mut step_timer) in player_query.iter_mut() {
        let cam_transform = camera.single()?;
        let curved_input = apply_response_curve(*navigate, MOVEMENT_CURVE_EXPONENT);
        let direction = cam_transform.movement_direction(curved_input);

        let float_height = 0.15;
        controller.basis(TnuaBuiltinWalk {
            float_height,
            cling_distance: float_height + 0.05,
            spring_strength: 500.0,
            spring_dampening: 1.0,
            acceleration: 80.0,
            air_acceleration: 50.0,
            free_fall_extra_gravity: 70.0,
            tilt_offset_angvel: 7.0,
            tilt_offset_angacl: 700.0,
            turning_angvel: 12.0,
            desired_velocity: direction * player.speed,
            desired_forward: Dir3::new(direction).ok(),
            ..Default::default()
        });

        // Check if crouch is currently active and apply TnuaBuiltinCrouch as an action
        if *crouch {
            controller.action(TnuaBuiltinCrouch {
                float_offset: 0.0,
                height_change_impulse_for_duration: 0.1,
                height_change_impulse_limit: 80.0,
                uncancellable: false,
            });
        }

        // update step timer dynamically based on actual speed
        // Note: this is specific to the animation provided
        // normal step: 0.475
        // sprint step (x1.5): 0.354
        // step on sprint timer: 0.317
        let Some((_, basis_state)) = controller.concrete_basis::<TnuaBuiltinWalk>() else {
            return Ok(());
        };
        let current_actual_speed = basis_state.running_velocity.length();
        if current_actual_speed > IDLE_TO_RUN_TRESHOLD {
            let ratio = cfg.player.movement.speed / current_actual_speed;
            let adjusted_step_time_f32 = cfg.timers.step * ratio;
            let adjusted_step_time = Duration::from_secs_f32(adjusted_step_time_f32);
            // info!("step timer:{adjusted_step_time_f32}s");
            step_timer.set_duration(adjusted_step_time);
        }
    }

    Ok(())
}

/// Check if player is grounded (for input buffering)
fn is_grounded(controller: &TnuaController) -> bool {
    controller
        .concrete_basis::<TnuaBuiltinWalk>()
        .is_some_and(|(_, state)| state.standing_on_entity().is_some())
}

fn handle_jump(
    on: On<Fire<Jump>>,
    mut buffer: ResMut<InputBuffer>,
    mut player_query: Query<
        (
            &mut TnuaController,
            &mut TnuaSimpleAirActionsCounter,
            &mut JumpTimer,
        ),
        With<Player>,
    >,
) -> Result {
    let (mut controller, mut air_counter, mut _jump_timer) = player_query.get_mut(on.context)?;

    // If not grounded and no air jumps left, buffer the input
    air_counter.update(controller.as_mut());
    let grounded = is_grounded(&controller);

    if !grounded {
        // Buffer for when we land
        buffer.buffer_jump();
    }

    // Still attempt the jump (Tnua will reject if invalid)
    controller.action(jump_action());
    Ok(())
}

/// Execute buffered jump when landing
fn process_buffered_jump(
    mut buffer: ResMut<InputBuffer>,
    mut player_query: Query<(&mut TnuaController, &mut TnuaSimpleAirActionsCounter), With<Player>>,
) {
    if buffer.jump.is_none() {
        return;
    }

    let Ok((mut controller, mut air_counter)) = player_query.single_mut() else {
        return;
    };

    // Only execute if we just landed (grounded now)
    if !is_grounded(&controller) {
        return;
    }

    // Clear buffer and execute jump
    buffer.jump = None;
    air_counter.update(controller.as_mut());
    controller.action(jump_action());
}

fn handle_dash(
    on: On<Start<Dash>>,
    cfg: Res<Config>,
    mut buffer: ResMut<InputBuffer>,
    navigate: Single<&Action<Navigate>>,
    camera: Query<&Transform, With<SceneCamera>>,
    mut player_query: Query<(
        &mut TnuaController,
        &TnuaSimpleAirActionsCounter,
        Option<&mut AttackState>,
    )>,
) -> Result {
    let (mut controller, air_counter, attack_state) = player_query.get_mut(on.context)?;

    let grounded = is_grounded(&controller);
    let air_dashes_allowed = cfg.player.movement.actions_in_air as usize;
    let can_air_dash = air_counter.air_count_for(TnuaBuiltinDash::NAME) <= air_dashes_allowed;

    // Buffer if we can't dash right now (in air and over limit)
    if !grounded && !can_air_dash {
        buffer.buffer_dash();
        return Ok(());
    }

    // Dash cancels any active attack
    if let Some(mut attack) = attack_state {
        if attack.attacking {
            attack.attacking = false;
        }
    }

    let cam_transform = camera.single()?;
    let navigate = **navigate.into_inner();
    let direction = cam_transform.movement_direction(navigate);

    controller.action(TnuaBuiltinDash {
        speed: 12.,
        displacement: direction * cfg.player.movement.dash_distance,
        desired_forward: Dir3::new(direction).ok(),
        allow_in_air: can_air_dash,
        ..Default::default()
    });

    Ok(())
}

/// Execute buffered dash when landing
fn process_buffered_dash(
    cfg: Res<Config>,
    mut buffer: ResMut<InputBuffer>,
    navigate: Query<&Action<Navigate>>,
    camera: Query<&Transform, With<SceneCamera>>,
    mut player_query: Query<(&mut TnuaController, Option<&mut AttackState>), With<Player>>,
) {
    if buffer.dash.is_none() {
        return;
    }

    let Ok((mut controller, attack_state)) = player_query.single_mut() else {
        return;
    };

    if !is_grounded(&controller) {
        return;
    }

    // Clear buffer
    buffer.dash = None;

    // Cancel attack if active
    if let Some(mut attack) = attack_state {
        if attack.attacking {
            attack.attacking = false;
        }
    }

    let Ok(nav_action) = navigate.single() else {
        return;
    };
    let Ok(cam_transform) = camera.single() else {
        return;
    };

    let nav = **nav_action;
    let direction = cam_transform.movement_direction(nav);

    controller.action(TnuaBuiltinDash {
        speed: 12.,
        displacement: direction * cfg.player.movement.dash_distance,
        desired_forward: Dir3::new(direction).ok(),
        allow_in_air: true, // We're grounded, doesn't matter
        ..Default::default()
    });
}

pub fn crouch_in(
    on: On<Start<Crouch>>,
    cfg: Res<Config>,
    mut player: Query<&mut Player, With<PlayerCtx>>,
    mut tnua: Query<(&mut TnuaAvian3dSensorShape, &mut Collider), With<Player>>,
) -> Result {
    let (mut avian_sensor, mut collider) = tnua.single_mut()?;
    let mut player = player.get_mut(on.context)?;

    collider.set_scale(Vec3::new(1.0, 0.5, 1.0), 4);
    avian_sensor.0.set_scale(Vec3::new(1.0, 0.5, 1.0), 4);
    player.speed *= cfg.player.movement.crouch_factor;

    Ok(())
}

pub fn crouch_out(
    on: On<Complete<Crouch>>,
    cfg: Res<Config>,
    mut player: Query<&mut Player, With<PlayerCtx>>,
    mut tnua: Query<
        (&mut TnuaAvian3dSensorShape, &mut Collider),
        (With<Player>, Without<SceneCamera>),
    >,
) -> Result {
    let (mut avian_sensor, mut collider) = tnua.get_mut(on.context)?;
    let mut player = player.get_mut(on.context)?;

    collider.set_scale(Vec3::ONE, 4);
    avian_sensor.0.set_scale(Vec3::ONE, 4);
    player.speed = cfg.player.movement.speed;

    Ok(())
}

use super::*;
use bevy_tnua::{
    builtins::{TnuaBuiltinCrouch, TnuaBuiltinDash},
    control_helpers::TnuaSimpleAirActionsCounter,
};

pub const IDLE_TO_RUN_TRESHOLD: f32 = 0.01;

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        movement
            .in_set(TnuaUserControlsSystems)
            .run_if(in_state(Screen::Gameplay)),
    )
    .add_observer(handle_jump)
    .add_observer(handle_dash)
    .add_observer(crouch_in)
    .add_observer(crouch_out);
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
        let direction = cam_transform.movement_direction(*navigate);

        let float_height = 0.15; // Lower to reduce hovering
        controller.basis(TnuaBuiltinWalk {
            float_height,
            cling_distance: float_height + 0.05,
            spring_strength: 500.0, // Stronger spring for a more grounded feel.
            spring_dampening: 1.0,  // Slightly reduced dampening for a more responsive spring.
            acceleration: 80.0, // Increased acceleration for snappier movement starts and stops.
            air_acceleration: 50.0, // Good air control for jumping over enemies
            free_fall_extra_gravity: 70.0, // Slightly increased for a less floaty fall.
            tilt_offset_angvel: 7.0, // Increased for a slightly faster righting response.
            tilt_offset_angacl: 700.0, // Increased acceleration to reach the target righting speed.
            turning_angvel: 12.0, // Increased for more responsive turning.
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

fn handle_jump(
    on: On<Fire<Jump>>,
    // cfg: Res<Config>,
    // time: Res<Time>,
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

    // if jump_timer.tick(time.delta()).just_finished() {
    air_counter.update(controller.as_mut()); // Update air counter
    controller.action(TnuaBuiltinJump {
        height: 5.0,                 // Higher to clear enemies
        takeoff_extra_gravity: 40.0, // Slightly reduced for more hang time
        fall_extra_gravity: 35.0,    // Slightly reduced for better air control
        shorten_extra_gravity: 80.0, // Keep short hops possible
        peak_prevention_at_upward_velocity: 0.5,
        peak_prevention_extra_gravity: 25.0, // Reduced for more air time at apex
        reschedule_cooldown: Some(0.1),
        disable_force_forward_after_peak: true,
        allow_in_air: true,
        ..Default::default()
    });
    // }

    Ok(())
}

fn handle_dash(
    on: On<Start<Dash>>,
    cfg: Res<Config>,
    navigate: Single<&Action<Navigate>>,
    camera: Query<&Transform, With<SceneCamera>>,
    mut player_query: Query<(
        &mut TnuaController,
        &TnuaSimpleAirActionsCounter,
        Option<&mut AttackState>,
    )>,
) -> Result {
    let (mut controller, air_counter, attack_state) = player_query.get_mut(on.context)?;

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
        speed: 12., // Tuned for slide animation timing
        displacement: direction * cfg.player.movement.dash_distance,
        desired_forward: Dir3::new(direction).ok(),
        allow_in_air: air_counter.air_count_for(TnuaBuiltinDash::NAME)
            <= cfg.player.movement.actions_in_air.into(),
        ..Default::default()
    });

    Ok(())
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

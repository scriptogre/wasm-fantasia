use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_third_person_camera::CameraSyncSet;

use crate::models::{Config, Player, SceneCamera, Screen};
use crate::player::control::{AirborneTracker, JumpCharge, LandingStun, Sprinting};

/// Tracks dynamic FOV state for smooth interpolation.
#[derive(Resource)]
pub struct DynamicFov {
    current: f32, // radians
    base: f32,    // radians (from config)
}

impl Default for DynamicFov {
    fn default() -> Self {
        Self {
            current: 75_f32.to_radians(),
            base: 75_f32.to_radians(),
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<DynamicFov>().add_systems(
        PostUpdate,
        (dynamic_fov, sprint_micro_shake, fall_camera_dip)
            .after(CameraSyncSet)
            .before(TransformSystems::Propagate)
            .run_if(in_state(Screen::Gameplay)),
    );
}

/// Max downward velocity for scaling fall effects.
const FALL_MAX_VELOCITY: f32 = 25.0;

fn dynamic_fov(
    time: Res<Time>,
    cfg: Res<Config>,
    mut fov_state: ResMut<DynamicFov>,
    player: Query<
        (
            &bevy_tnua::prelude::TnuaController<crate::player::ControlScheme>,
            &LinearVelocity,
            &JumpCharge,
            Has<Sprinting>,
            Option<&LandingStun>,
        ),
        With<Player>,
    >,
    mut camera: Query<&mut Projection, With<SceneCamera>>,
) {
    let Ok((controller, velocity, jump_charge, is_sprinting, landing_stun)) = player.single()
    else {
        return;
    };
    let Ok(mut projection) = camera.single_mut() else {
        return;
    };

    // Lazily sync base from config
    let config_fov = cfg.player.fov.to_radians();
    if (fov_state.base - config_fov).abs() > 0.001 {
        fov_state.base = config_fov;
    }

    let speed = controller.basis_memory.running_velocity.length();
    let sprint_speed = cfg.player.movement.speed * cfg.player.movement.sprint_factor;
    let idle_threshold = cfg.player.movement.idle_to_run_threshold;

    let mut target = fov_state.base;

    if is_sprinting && speed > idle_threshold {
        // Sprint FOV: scales with how close to sprint max speed, up to +10 degrees
        let sprint_ratio = (speed / sprint_speed).clamp(0.0, 1.0);
        target += 10_f32.to_radians() * sprint_ratio;
    }

    // Airborne at high speed: keep FOV expanded based on velocity
    let grounded = controller.basis_memory.standing_on_entity().is_some();
    if !grounded {
        let air_speed_ratio = (speed / sprint_speed).clamp(0.0, 1.0);
        if air_speed_ratio > 0.3 {
            target += 6_f32.to_radians() * air_speed_ratio;
        }

        // Falling: widen FOV based on downward velocity — gut-drop feeling
        let fall_speed = (-velocity.y).max(0.0);
        if fall_speed > 3.0 {
            let fall_t = ((fall_speed - 3.0) / (FALL_MAX_VELOCITY - 3.0)).clamp(0.0, 1.0);
            target += 12_f32.to_radians() * fall_t;
        }
    }

    // Jump charge: narrow FOV by 3 degrees (anticipation)
    if jump_charge.charging {
        let charge_t =
            (jump_charge.charge_time / crate::player::control::MAX_CHARGE_TIME).clamp(0.0, 1.0);
        target -= 3_f32.to_radians() * charge_t;
    }

    // Landing stun: FOV dip on impact, smoothly recovers as stun wears off
    if let Some(stun) = landing_stun {
        let impact_strength = 1.0 - stun.timer.fraction();
        target -= 5_f32.to_radians() * impact_strength;
    }

    // Smooth interpolation — fast expand on launch/fall, slower return to base
    let dt = time.delta_secs();
    let lerp_speed = if target > fov_state.current {
        10.0 // Fast expand (launch, fall)
    } else {
        4.0 // Slower contract
    };
    fov_state.current += (target - fov_state.current) * (lerp_speed * dt).min(1.0);

    if let Projection::Perspective(ref mut persp) = *projection {
        persp.fov = fov_state.current;
    }
}

/// Camera Y dip when falling — simulates the gut-drop weight of a fall.
/// Pushes camera slightly downward relative to the player, scaling with fall speed.
fn fall_camera_dip(
    player: Query<
        (&LinearVelocity, &AirborneTracker),
        With<Player>,
    >,
    mut camera: Query<&mut Transform, With<SceneCamera>>,
) {
    let Ok((velocity, tracker)) = player.single() else {
        return;
    };
    let Ok(mut cam_transform) = camera.single_mut() else {
        return;
    };

    if !tracker.was_airborne {
        return;
    }

    // Downward camera dip proportional to fall speed
    let fall_speed = (-velocity.y).max(0.0);
    if fall_speed > 3.0 {
        let fall_t = ((fall_speed - 3.0) / (FALL_MAX_VELOCITY - 3.0)).clamp(0.0, 1.0);
        // Push camera down by up to 1.5 units at max fall speed
        cam_transform.translation.y -= 1.5 * fall_t * fall_t; // Quadratic for acceleration feel
    }
}

fn sprint_micro_shake(
    time: Res<Time>,
    cfg: Res<Config>,
    player: Query<
        (
            &bevy_tnua::prelude::TnuaController<crate::player::ControlScheme>,
            Has<Sprinting>,
        ),
        With<Player>,
    >,
    mut camera: Query<&mut Transform, With<SceneCamera>>,
) {
    let Ok((controller, is_sprinting)) = player.single() else {
        return;
    };
    if !is_sprinting {
        return;
    }
    let Ok(mut transform) = camera.single_mut() else {
        return;
    };

    let speed = controller.basis_memory.running_velocity.length();
    let sprint_speed = cfg.player.movement.speed * cfg.player.movement.sprint_factor;
    let speed_ratio = (speed / sprint_speed).clamp(0.0, 1.0);

    // Only shake when actually moving while sprinting
    if speed_ratio < 0.3 {
        return;
    }

    let amplitude = 0.02 + 0.04 * speed_ratio;
    let t = time.elapsed_secs();

    // Multiple sine waves for organic procedural noise
    let x = (t * 8.3).sin() * 0.5 + (t * 17.1).cos() * 0.3 + (t * 23.7).sin() * 0.2;
    let y = (t * 11.7).cos() * 0.5 + (t * 19.3).sin() * 0.3 + (t * 29.1).cos() * 0.2;

    transform.translation.x += x * amplitude;
    transform.translation.y += y * amplitude;
}

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_third_person_camera::CameraSyncSet;

use crate::models::{Config, Player, SceneCamera, Screen};
use crate::player::control::{AirborneTracker, LandingImpact, LandingStun, Sprinting};

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

/// Screen shake triggered by landing impacts — high-frequency sine with quadratic decay.
#[derive(Resource, Default)]
pub struct ScreenShake {
    intensity: f32,
    duration: f32,
    timer: f32,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<DynamicFov>()
        .init_resource::<ScreenShake>()
        .add_observer(on_landing_shake)
        .add_systems(
            PostUpdate,
            (dynamic_fov, fall_camera_dip, apply_screen_shake)
                .after(CameraSyncSet)
                .before(TransformSystems::Propagate)
                .run_if(in_state(Screen::Gameplay)),
        );
}

/// Max downward velocity for scaling fall effects.
const FALL_MAX_VELOCITY: f32 = 25.0;

/// SmoothStep helper for FOV scaling
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn dynamic_fov(
    time: Res<Time>,
    cfg: Res<Config>,
    mut fov_state: ResMut<DynamicFov>,
    player: Query<
        (
            &bevy_tnua::prelude::TnuaController<crate::player::ControlScheme>,
            &LinearVelocity,
            Has<Sprinting>,
            Option<&LandingStun>,
        ),
        With<Player>,
    >,
    mut camera: Query<&mut Projection, With<SceneCamera>>,
) {
    let Ok((controller, velocity, is_sprinting, landing_stun)) = player.single() else {
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
        // Sprint FOV: +14° with smoothstep — enough tunnel vision to sell speed without nausea
        let sprint_ratio = (speed / sprint_speed).clamp(0.0, 1.0);
        target += 14_f32.to_radians() * smoothstep(sprint_ratio);
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
            target += 18_f32.to_radians() * fall_t;
        }
    }

    // Landing stun: FOV punch on impact (12°), smoothly recovers as stun wears off
    if let Some(stun) = landing_stun {
        let impact_strength = 1.0 - stun.timer.fraction();
        target -= 12_f32.to_radians() * impact_strength;
    }

    // Smooth interpolation — slower expand builds tension, fast contract snaps on stop
    let dt = time.delta_secs();
    let lerp_speed = if target > fov_state.current {
        6.0 // Gradual expand — feel the speed building
    } else {
        8.0 // Faster contract — sudden stop feels jarring/impactful
    };
    fov_state.current += (target - fov_state.current) * (lerp_speed * dt).min(1.0);

    if let Projection::Perspective(ref mut persp) = *projection {
        persp.fov = fov_state.current;
    }
}

/// Camera Y dip when falling — simulates the gut-drop weight of a fall.
/// Pushes camera slightly downward relative to the player, scaling with fall speed.
fn fall_camera_dip(
    player: Query<(&LinearVelocity, &AirborneTracker), With<Player>>,
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
        // Push camera down by up to 2.0 units at max fall speed — gut-drop weight
        cam_transform.translation.y -= 2.0 * fall_t * fall_t; // Quadratic for acceleration feel
    }
}

// ── Screen Shake ────────────────────────────────────────────────────

fn on_landing_shake(on: On<LandingImpact>, mut shake: ResMut<ScreenShake>) {
    let event = on.event();
    let t = ((event.velocity_y - 3.0) / 22.0).clamp(0.0, 1.0);

    // Quadratic intensity scaling — heavy falls shake hard (max 0.25 at terminal velocity)
    shake.intensity = 0.04 + 0.21 * t * t;
    shake.duration = 0.2 + 0.25 * t;
    shake.timer = 0.0;
}

fn apply_screen_shake(
    time: Res<Time>,
    mut shake: ResMut<ScreenShake>,
    mut camera: Query<&mut Transform, With<SceneCamera>>,
) {
    if shake.duration <= 0.0 || shake.timer >= shake.duration {
        return;
    }

    shake.timer += time.delta_secs();
    let t = (shake.timer / shake.duration).min(1.0);

    // Quadratic decay
    let decay = (1.0 - t) * (1.0 - t);
    let elapsed = shake.timer;

    // High-frequency sine waves for punchy shake
    let x = (elapsed * 45.0).sin() * 0.6 + (elapsed * 73.0).cos() * 0.4;
    let y = (elapsed * 53.0).cos() * 0.6 + (elapsed * 67.0).sin() * 0.4;

    let Ok(mut cam_transform) = camera.single_mut() else {
        return;
    };

    cam_transform.translation.x += x * shake.intensity * decay;
    cam_transform.translation.y += y * shake.intensity * decay;
}

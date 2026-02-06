//! Camera assist systems: target lock, auto-center, and reset.
//!
//! These run AFTER bevy_third_person_camera to adjust rotation without
//! fighting the library's orbit controls.

use bevy::input::gamepad::GamepadAxisChangedEvent;
use bevy::prelude::*;
use bevy_third_person_camera::{CameraSyncSet, ThirdPersonCamera};

use crate::combat::{AttackState, Enemy, LockedTarget};
use crate::models::{Navigate, Player, SceneCamera, Screen};

pub fn plugin(app: &mut App) {
    app.init_resource::<CameraAssist>().add_systems(
        PostUpdate,
        (
            target_lock_camera.after(CameraSyncSet),
            auto_center_camera.after(CameraSyncSet),
            track_camera_input,
        )
            .run_if(in_state(Screen::Gameplay)),
    );
}

/// Camera assist state
#[derive(Resource)]
pub struct CameraAssist {
    /// Time since last camera stick input (seconds)
    pub idle_time: f32,
    /// How long before auto-center kicks in
    pub auto_center_delay: f32,
    /// Auto-center rotation speed (radians/sec)
    pub auto_center_speed: f32,
    /// Target lock rotation speed (radians/sec)
    pub target_lock_speed: f32,
    /// Whether target lock is enabled
    pub target_lock_enabled: bool,
    /// Whether auto-center is enabled
    pub auto_center_enabled: bool,
}

impl Default for CameraAssist {
    fn default() -> Self {
        Self {
            idle_time: 0.0,
            auto_center_delay: 3.0,         // Wait 3 seconds before any nudging
            auto_center_speed: 0.2,          // Very slow drift
            target_lock_speed: 0.4,          // Subtle target tracking
            target_lock_enabled: true,
            auto_center_enabled: true,
        }
    }
}

/// Track camera stick input to know when player is actively controlling
fn track_camera_input(
    time: Res<Time>,
    mut assist: ResMut<CameraAssist>,
    mut axis_events: MessageReader<GamepadAxisChangedEvent>,
) {
    // Check for right stick input (camera control)
    let has_camera_input = axis_events.read().any(|e| {
        matches!(
            e.axis,
            bevy::input::gamepad::GamepadAxis::RightStickX
                | bevy::input::gamepad::GamepadAxis::RightStickY
        ) && e.value.abs() > 0.1
    });

    if has_camera_input {
        assist.idle_time = 0.0;
    } else {
        assist.idle_time += time.delta_secs();
    }
}

/// When locked onto a target, rotate camera to keep both player and target in frame.
/// Only activates when player is actively attacking - not just standing near enemies.
fn target_lock_camera(
    time: Res<Time>,
    assist: Res<CameraAssist>,
    target: Res<LockedTarget>,
    player: Query<(&GlobalTransform, Option<&AttackState>), With<Player>>,
    enemies: Query<&GlobalTransform, (With<Enemy>, Without<Player>)>,
    mut camera: Query<
        &mut Transform,
        (
            With<ThirdPersonCamera>,
            With<SceneCamera>,
            Without<Player>,
            Without<Enemy>,
        ),
    >,
) {
    if !assist.target_lock_enabled {
        return;
    }

    // Only engage when we have a locked target
    let Some(target_entity) = target.get() else {
        return;
    };

    // Only assist camera when player is actively attacking
    let Ok((player_global, attack_state)) = player.single() else {
        return;
    };
    let is_attacking = attack_state.is_some_and(|a| a.attacking);
    if !is_attacking {
        return;
    }

    let Ok(target_tf) = enemies.get(target_entity) else {
        return;
    };

    let Ok(mut camera_tf) = camera.single_mut() else {
        return;
    };

    let player_pos = player_global.translation();
    let target_pos = target_tf.translation();

    // Camera should look toward a point that keeps both in frame
    // Position camera to view the midpoint between player and target
    let midpoint = (player_pos + target_pos) / 2.0;
    let midpoint_flat = Vec3::new(midpoint.x, player_pos.y, midpoint.z);

    // Direction from camera to midpoint (horizontal only)
    let camera_pos_flat = Vec3::new(camera_tf.translation.x, player_pos.y, camera_tf.translation.z);
    let to_midpoint = midpoint_flat - camera_pos_flat;

    if to_midpoint.length_squared() < 0.01 {
        return;
    }

    // Calculate desired camera rotation (horizontal only, preserve pitch)
    let current_forward = camera_tf.forward().as_vec3();
    let current_forward_flat = Vec3::new(current_forward.x, 0.0, current_forward.z).normalize();

    let desired_forward = to_midpoint.normalize();

    // Slerp between current and desired horizontal direction
    let current_rot_y = current_forward_flat.x.atan2(-current_forward_flat.z);
    let desired_rot_y = desired_forward.x.atan2(-desired_forward.z);

    // Shortest path rotation
    let mut delta = desired_rot_y - current_rot_y;
    if delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    } else if delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }

    // Apply rotation smoothly
    let max_rotation = assist.target_lock_speed * time.delta_secs();
    let rotation = delta.clamp(-max_rotation, max_rotation);

    camera_tf.rotate_around(player_pos, Quat::from_rotation_y(rotation));
}

/// When moving with no camera input, gradually rotate camera behind player.
/// Only activates after a delay to avoid fighting intentional camera positioning.
fn auto_center_camera(
    time: Res<Time>,
    assist: Res<CameraAssist>,
    target: Res<LockedTarget>,
    navigate: Query<&bevy_enhanced_input::prelude::Action<Navigate>>,
    player: Query<&GlobalTransform, With<Player>>,
    mut camera: Query<
        &mut Transform,
        (
            With<ThirdPersonCamera>,
            With<SceneCamera>,
            Without<Player>,
        ),
    >,
) {
    if !assist.auto_center_enabled {
        return;
    }

    // Don't auto-center if target locked (target lock takes priority)
    if target.is_locked() {
        return;
    }

    // Don't auto-center until idle delay has passed
    if assist.idle_time < assist.auto_center_delay {
        return;
    }

    // Check if player is moving
    let Ok(nav_action) = navigate.single() else {
        return;
    };

    let nav_value = **nav_action;
    if nav_value.length_squared() < 0.1 {
        return; // Not moving, don't center
    }

    let Ok(player_tf) = player.single() else {
        return;
    };

    let Ok(mut camera_tf) = camera.single_mut() else {
        return;
    };

    let player_pos = player_tf.translation();

    // Get player's movement direction in world space
    // Camera should rotate to be behind this direction
    let camera_forward = camera_tf.forward().as_vec3();
    let camera_right = camera_tf.right().as_vec3();

    // Convert navigation input to world direction (relative to camera)
    let move_dir_world = (camera_forward * -nav_value.y + camera_right * nav_value.x).normalize();
    let move_dir_flat = Vec3::new(move_dir_world.x, 0.0, move_dir_world.z).normalize();

    // Current camera forward (horizontal)
    let current_forward = camera_forward;
    let current_forward_flat = Vec3::new(current_forward.x, 0.0, current_forward.z).normalize();

    // We want camera looking in the same direction as movement (behind player)
    let desired_forward = -move_dir_flat; // Camera looks opposite to movement = behind player

    let current_rot_y = current_forward_flat.x.atan2(-current_forward_flat.z);
    let desired_rot_y = desired_forward.x.atan2(-desired_forward.z);

    let mut delta = desired_rot_y - current_rot_y;
    if delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    } else if delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }

    // Ease in based on how long we've been idle (smoother start)
    let idle_factor = ((assist.idle_time - assist.auto_center_delay) / 0.5).min(1.0);
    let speed = assist.auto_center_speed * idle_factor;

    let max_rotation = speed * time.delta_secs();
    let rotation = delta.clamp(-max_rotation, max_rotation);

    camera_tf.rotate_around(player_pos, Quat::from_rotation_y(rotation));
}


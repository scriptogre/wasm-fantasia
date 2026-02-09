use super::*;
use avian3d::prelude::LinearVelocity;

/// Separation force configuration
const SEPARATION_DISTANCE: f32 = 1.2;
const SEPARATION_FORCE: f32 = 8.0;

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        apply_separation_force
            .run_if(in_state(Screen::Gameplay))
            .run_if(not(is_paused)),
    );
}

/// Apply separation force to prevent player from climbing on enemies.
/// Pushes player away when too close. Force is capped to prevent extreme knockback.
fn apply_separation_force(
    mut player: Query<&mut LinearVelocity, (With<Player>, Without<Enemy>)>,
    enemies: Query<&Transform, With<Enemy>>,
    player_tf: Query<&Transform, With<Player>>,
) {
    let Ok(player_transform) = player_tf.single() else {
        return;
    };
    let Ok(mut player_vel) = player.single_mut() else {
        return;
    };

    let player_pos = player_transform.translation;

    // Accumulate total push from all enemies, then cap it
    let mut total_push = Vec3::ZERO;

    for enemy_tf in enemies.iter() {
        let enemy_pos = enemy_tf.translation;

        // Only apply separation when player is at roughly the same height as enemy
        let height_diff = (player_pos.y - enemy_pos.y).abs();
        if height_diff > 1.5 {
            continue;
        }

        let horizontal_diff =
            Vec3::new(player_pos.x - enemy_pos.x, 0.0, player_pos.z - enemy_pos.z);
        let horizontal_distance = horizontal_diff.length();

        if horizontal_distance < SEPARATION_DISTANCE && horizontal_distance > 0.01 {
            let push_dir = horizontal_diff.normalize();
            // Gentler falloff: square root for smoother push
            let t = 1.0 - (horizontal_distance / SEPARATION_DISTANCE);
            let push_strength = t.sqrt() * SEPARATION_FORCE * 0.5;

            total_push += push_dir * push_strength;
        }
    }

    // Cap total push force to prevent extreme knockback from multiple enemies
    let max_push = SEPARATION_FORCE * 0.5;
    if total_push.length() > max_push {
        total_push = total_push.normalize() * max_push;
    }

    // Apply as a gentle nudge, not a hard push
    player_vel.x += total_push.x * 0.3;
    player_vel.z += total_push.z * 0.3;
}

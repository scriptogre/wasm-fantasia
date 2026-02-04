use super::*;
use bevy_enhanced_input::prelude::Start;
use crate::models::TargetLock;

/// Maximum distance for target lock
pub const LOCK_RANGE: f32 = 20.0;
/// Distance at which lock breaks
pub const LOCK_BREAK_RANGE: f32 = 25.0;

pub fn plugin(app: &mut App) {
    app.init_resource::<LockedTarget>()
        .add_observer(cycle_target)
        .add_systems(
            Update,
            (
                update_lock_indicators,
                rotate_player_to_target,
                break_lock_on_range,
                break_lock_on_death,
            )
                .run_if(in_state(Screen::Gameplay)),
        );
}

/// Resource tracking the currently locked target
#[derive(Resource, Default, Debug)]
pub struct LockedTarget(pub Option<Entity>);

impl LockedTarget {
    pub fn get(&self) -> Option<Entity> {
        self.0
    }

    pub fn set(&mut self, target: Option<Entity>) {
        self.0 = target;
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }

    pub fn is_locked(&self) -> bool {
        self.0.is_some()
    }
}

/// Marker for the lock indicator above target
#[derive(Component)]
pub struct LockIndicatorAbove;

/// Marker for the lock indicator ring below target
#[derive(Component)]
pub struct LockIndicatorBelow;

/// Cycle through valid targets or unlock
fn cycle_target(
    _on: On<Start<TargetLock>>,
    mut locked: ResMut<LockedTarget>,
    player: Query<&Transform, With<Player>>,
    targets: Query<(Entity, &Transform), With<Enemy>>,
) {
    let Ok(player_tf) = player.single() else {
        return;
    };

    // Gather valid targets within range, sorted by distance
    let mut valid_targets: Vec<(Entity, f32)> = targets
        .iter()
        .filter_map(|(e, tf)| {
            let dist = player_tf.translation.distance(tf.translation);
            if dist <= LOCK_RANGE {
                Some((e, dist))
            } else {
                None
            }
        })
        .collect();

    // Sort by distance (closest first)
    valid_targets.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    if valid_targets.is_empty() {
        // No valid targets, clear lock
        locked.clear();
        return;
    }

    match locked.get() {
        None => {
            // Lock onto closest target
            locked.set(Some(valid_targets[0].0));
        }
        Some(current) => {
            // Find current target's index
            let current_idx = valid_targets.iter().position(|(e, _)| *e == current);

            match current_idx {
                Some(idx) => {
                    // Cycle to next target, wrap to first if at end
                    let next_idx = (idx + 1) % valid_targets.len();
                    locked.set(Some(valid_targets[next_idx].0));
                }
                None => {
                    // Current target no longer valid, lock closest
                    locked.set(Some(valid_targets[0].0));
                }
            }
        }
    }
}

/// Spawn/update/despawn lock indicators
fn update_lock_indicators(
    locked: Res<LockedTarget>,
    targets: Query<&Transform, With<Enemy>>,
    mut indicators_above: Query<
        (Entity, &mut Transform),
        (With<LockIndicatorAbove>, Without<Enemy>, Without<LockIndicatorBelow>),
    >,
    mut indicators_below: Query<
        (Entity, &mut Transform),
        (With<LockIndicatorBelow>, Without<Enemy>, Without<LockIndicatorAbove>),
    >,
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    match locked.get() {
        Some(target_entity) => {
            let Ok(target_tf) = targets.get(target_entity) else {
                // Target no longer exists, will be cleaned up by break_lock_on_death
                return;
            };

            let target_pos = target_tf.translation;

            // Update or spawn indicator above (downward pointing triangle/arrow)
            if let Ok((_, mut tf)) = indicators_above.single_mut() {
                // Animate: bob up and down
                let bob = (time.elapsed_secs() * 3.0).sin() * 0.1;
                tf.translation = target_pos + Vec3::Y * (2.0 + bob);
            } else {
                // Cone pointing downward
                let mesh = meshes.add(Cone::new(0.15, 0.3));
                let material = materials.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 0.3, 0.3, 0.95),
                    emissive: LinearRgba::new(3.0, 0.6, 0.6, 1.0),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    cull_mode: None,
                    ..default()
                });

                commands.spawn((
                    LockIndicatorAbove,
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::from_translation(target_pos + Vec3::Y * 2.0)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::PI)), // Point downward
                ));
            }

            // Update or spawn indicator below (thin ring on ground)
            let ground_pos = Vec3::new(target_pos.x, 0.02, target_pos.z);

            if let Ok((_, mut tf)) = indicators_below.single_mut() {
                tf.translation = ground_pos;
                tf.rotate_y(time.delta_secs() * 1.5);
            } else {
                // Thin annulus (ring) - use a torus with very small tube radius
                let mesh = meshes.add(Annulus::new(0.7, 0.8));
                let material = materials.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 0.2, 0.2, 0.9),
                    emissive: LinearRgba::new(2.5, 0.5, 0.5, 1.0),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                });

                commands.spawn((
                    LockIndicatorBelow,
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::from_translation(ground_pos)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                ));
            }
        }
        None => {
            // No lock, despawn indicators
            for (e, _) in indicators_above.iter() {
                commands.entity(e).despawn();
            }
            for (e, _) in indicators_below.iter() {
                commands.entity(e).despawn();
            }
        }
    }
}

/// Smoothly rotate player to face locked target
fn rotate_player_to_target(
    locked: Res<LockedTarget>,
    targets: Query<&Transform, (With<Enemy>, Without<Player>)>,
    mut player: Query<(&mut Transform, &AttackState), With<Player>>,
    time: Res<Time>,
) {
    let Some(target_entity) = locked.get() else {
        return;
    };

    let Ok(target_tf) = targets.get(target_entity) else {
        return;
    };

    let Ok((mut player_tf, attack_state)) = player.single_mut() else {
        return;
    };

    // Calculate direction to target (ignore Y)
    let to_target = target_tf.translation - player_tf.translation;
    let to_target_flat = Vec3::new(to_target.x, 0.0, to_target.z);

    if to_target_flat.length_squared() < 0.01 {
        return;
    }

    let target_rotation = Quat::from_rotation_arc(Vec3::NEG_Z, to_target_flat.normalize());

    // Fast rotation when attacking to ensure punches hit, smoother otherwise
    let rotation_speed = if attack_state.attacking { 25.0 } else { 12.0 };
    player_tf.rotation = player_tf
        .rotation
        .slerp(target_rotation, time.delta_secs() * rotation_speed);
}

/// Break lock when target goes out of range
fn break_lock_on_range(
    mut locked: ResMut<LockedTarget>,
    player: Query<&Transform, With<Player>>,
    targets: Query<&Transform, With<Enemy>>,
) {
    let Some(target_entity) = locked.get() else {
        return;
    };

    let Ok(player_tf) = player.single() else {
        return;
    };

    let Ok(target_tf) = targets.get(target_entity) else {
        // Target doesn't exist anymore
        locked.clear();
        return;
    };

    let distance = player_tf.translation.distance(target_tf.translation);
    if distance > LOCK_BREAK_RANGE {
        locked.clear();
    }
}

/// Break lock when target dies, auto-lock next if available
fn break_lock_on_death(
    mut locked: ResMut<LockedTarget>,
    player: Query<&Transform, With<Player>>,
    targets: Query<(Entity, &Transform), With<Enemy>>,
) {
    let Some(target_entity) = locked.get() else {
        return;
    };

    // Check if target still exists
    if targets.get(target_entity).is_ok() {
        return;
    }

    // Target died, try to lock next closest
    let Ok(player_tf) = player.single() else {
        locked.clear();
        return;
    };

    let next_target = targets
        .iter()
        .filter_map(|(e, tf)| {
            let dist = player_tf.translation.distance(tf.translation);
            if dist <= LOCK_RANGE {
                Some((e, dist))
            } else {
                None
            }
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(e, _)| e);

    locked.set(next_target);
}

use super::*;

/// Range for showing target indicator and soft targeting
pub const INDICATOR_RANGE: f32 = 8.0;

pub fn plugin(app: &mut App) {
    app.init_resource::<LockedTarget>().add_systems(
        Update,
        (update_target_indicator, soft_target_assist).run_if(in_state(Screen::Gameplay)),
    );
}

/// Resource tracking suggested target (for visual feedback only, no gameplay effect)
#[derive(Resource, Default, Debug)]
pub struct LockedTarget(pub Option<Entity>);

impl LockedTarget {
    pub fn get(&self) -> Option<Entity> {
        self.0
    }

    pub fn set(&mut self, target: Option<Entity>) {
        self.0 = target;
    }

    pub fn is_locked(&self) -> bool {
        self.0.is_some()
    }
}

/// Marker for the target indicator ring
#[derive(Component)]
pub struct TargetIndicator;

/// Soft targeting assist - gently rotate player toward nearest target when attacking.
/// This allows circling enemies while still connecting hits.
fn soft_target_assist(
    suggested: Res<LockedTarget>,
    enemies: Query<&Transform, (With<Enemy>, Without<Player>)>,
    mut player: Query<(&mut Transform, &AttackState), (With<Player>, Without<Enemy>)>,
    time: Res<Time>,
) {
    // Only assist when attacking
    let Ok((mut player_tf, attack_state)) = player.single_mut() else {
        return;
    };

    if !attack_state.attacking {
        return;
    }

    // Get suggested target
    let Some(target_entity) = suggested.get() else {
        return;
    };

    let Ok(target_tf) = enemies.get(target_entity) else {
        return;
    };

    // Calculate direction to target (ignore Y)
    let to_target = target_tf.translation - player_tf.translation;
    let to_target_flat = Vec3::new(to_target.x, 0.0, to_target.z);

    if to_target_flat.length_squared() < 0.01 {
        return;
    }

    let target_rotation = Quat::from_rotation_arc(Vec3::NEG_Z, to_target_flat.normalize());

    // Gentle rotation assist - fast enough to help, not so fast it feels forced
    // Stronger early in attack (wind-up), weaker during recovery
    let assist_strength = if attack_state.progress() < 0.4 {
        15.0
    } else {
        8.0
    };

    player_tf.rotation = player_tf
        .rotation
        .slerp(target_rotation, time.delta_secs() * assist_strength);
}

/// Update target indicator to show nearest enemy in front of player.
/// This is visual feedback only - no forced rotation or gameplay lock.
fn update_target_indicator(
    mut suggested: ResMut<LockedTarget>,
    player: Query<&Transform, With<Player>>,
    enemies: Query<(Entity, &Transform), With<Enemy>>,
    mut indicator: Query<
        (Entity, &mut Transform),
        (With<TargetIndicator>, Without<Enemy>, Without<Player>),
    >,
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(player_tf) = player.single() else {
        return;
    };

    let player_forward = player_tf.forward().as_vec3();
    let player_forward_flat =
        Vec3::new(player_forward.x, 0.0, player_forward.z).normalize_or_zero();

    // Find best target: closest enemy roughly in front of player
    let best_target = enemies
        .iter()
        .filter_map(|(entity, tf)| {
            let to_enemy = tf.translation - player_tf.translation;
            let distance = to_enemy.length();

            if distance > INDICATOR_RANGE {
                return None;
            }

            let to_enemy_flat = Vec3::new(to_enemy.x, 0.0, to_enemy.z).normalize_or_zero();
            let dot = player_forward_flat.dot(to_enemy_flat);

            // Must be at least partially in front (dot > 0 means < 90 degrees)
            if dot > 0.0 {
                // Score: prefer closer enemies, with slight preference for centered ones
                let score = distance - dot * 2.0; // Lower is better
                Some((entity, tf.translation, score))
            } else {
                None
            }
        })
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

    // Update suggested target resource
    suggested.set(best_target.map(|(e, _, _)| e));

    match best_target {
        Some((_, target_pos, _)) => {
            let ground_pos = Vec3::new(target_pos.x, 0.02, target_pos.z);

            if let Ok((_, mut tf)) = indicator.single_mut() {
                // Update existing indicator
                tf.translation = ground_pos;
                tf.rotate_y(time.delta_secs() * 2.0);
            } else {
                // Spawn indicator
                let mesh = meshes.add(Annulus::new(0.6, 0.7));
                let material = materials.add(StandardMaterial {
                    base_color: crate::ui::colors::SAND_YELLOW.with_alpha(0.6),
                    emissive: LinearRgba::new(1.5, 1.0, 0.3, 1.0),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                });

                commands.spawn((
                    TargetIndicator,
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::from_translation(ground_pos)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                ));
            }
        }
        None => {
            // No valid target, despawn indicator
            for (entity, _) in indicator.iter() {
                commands.entity(entity).despawn();
            }
        }
    }
}

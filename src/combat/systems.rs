use super::*;
use avian3d::prelude::LinearVelocity;
use avian3d::spatial_query::{SpatialQuery, SpatialQueryFilter};
use bevy_enhanced_input::prelude::Start;
use bevy_tnua::prelude::*;

/// Attack configuration
pub const ATTACK_RANGE: f32 = 1.2;
pub const ATTACK_RADIUS: f32 = 0.5;
pub const ATTACK_DAMAGE: f32 = 25.0;
pub const ATTACK_KNOCKBACK: f32 = 3.0;

/// Critical hit configuration
pub const CRIT_CHANCE: f32 = 0.20; // 20%
pub const CRIT_MULTIPLIER: f32 = 2.0; // 200% damage
pub const CRIT_KNOCKBACK_MULTIPLIER: f32 = 1.5;

/// Separation force configuration
pub const SEPARATION_DISTANCE: f32 = 1.0;
pub const SEPARATION_FORCE: f32 = 8.0;

pub fn plugin(app: &mut App) {
    app.add_observer(handle_attack)
        .add_observer(on_attack_connect)
        .add_observer(on_damage)
        .add_observer(on_death)
        .add_systems(
            Update,
            (tick_attack_state, apply_separation_force).run_if(in_state(Screen::Gameplay)),
        );
}

/// Handle attack input - start the attack sequence.
fn handle_attack(
    on: On<Start<Attack>>,
    mut query: Query<&mut AttackState, With<PlayerCombatant>>,
) {
    let Ok(mut attack_state) = query.get_mut(on.context) else {
        return;
    };

    if attack_state.can_attack() {
        // Roll for crit
        let mut rng = rand::rng();
        let is_crit = rand::Rng::random::<f32>(&mut rng) < CRIT_CHANCE;

        attack_state.start_attack();
        attack_state.is_crit = is_crit;
    }
}

/// Tick attack state timers and frames.
fn tick_attack_state(time: Res<Time>, mut query: Query<&mut AttackState>) {
    for mut state in query.iter_mut() {
        state.cooldown.tick(time.delta());

        if state.attacking {
            state.attack_frame += 1;

            // Crit (hook) animation is longer than regular punches
            let max_frames = if state.is_crit { 45 } else { 35 };

            if state.attack_frame > max_frames {
                state.attacking = false;
                state.attack_frame = 0;
                state.is_crit = false;
            }
        }
    }
}

/// Observer: triggered by AttackConnect animation event when any attack visually connects.
/// Can hit multiple enemies at once.
fn on_attack_connect(
    _on: On<AttackConnect>,
    spatial: SpatialQuery,
    locked_target: Res<LockedTarget>,
    mut attackers: Query<(Entity, &mut AttackState, &Transform), With<PlayerCombatant>>,
    targets: Query<(Entity, &Transform), (With<Health>, With<Enemy>)>,
    mut commands: Commands,
) {
    for (attacker_entity, mut attack_state, transform) in attackers.iter_mut() {
        // Only process if attacking and hit not yet triggered
        if !attack_state.attacking || attack_state.hit_triggered {
            continue;
        }

        // Mark hit as triggered
        attack_state.hit_triggered = true;

        let is_crit = attack_state.is_crit;
        let forward = transform.forward();

        // Calculate damage and knockback based on crit
        let damage = if is_crit {
            ATTACK_DAMAGE * CRIT_MULTIPLIER
        } else {
            ATTACK_DAMAGE
        };
        let knockback = if is_crit {
            ATTACK_KNOCKBACK * CRIT_KNOCKBACK_MULTIPLIER
        } else {
            ATTACK_KNOCKBACK
        };
        let hitbox_mult = if is_crit { 1.2 } else { 1.0 };

        // Track which entities we've hit to avoid duplicates
        let mut hit_entities = Vec::new();

        // If we have a locked target, always try to hit them with extended range
        if let Some(locked_entity) = locked_target.get() {
            if let Ok((_, target_tf)) = targets.get(locked_entity) {
                let to_target = target_tf.translation - transform.translation;
                let distance = to_target.length();

                // Extended range for locked target
                if distance <= ATTACK_RANGE * 2.5 {
                    let knockback_dir = to_target.normalize();

                    commands.trigger(DamageEvent {
                        target: locked_entity,
                        damage,
                        knockback_direction: knockback_dir,
                        knockback_force: knockback,
                        is_crit,
                    });
                    hit_entities.push(locked_entity);
                }
            }
        }

        // Also hit any other enemies in the attack hitbox
        let attack_pos = transform.translation + forward * ATTACK_RANGE * 0.5;

        let shape = Collider::sphere(ATTACK_RADIUS * hitbox_mult);
        let hits = spatial.shape_intersections(
            &shape,
            attack_pos,
            Quat::IDENTITY,
            &SpatialQueryFilter::default().with_excluded_entities([attacker_entity]),
        );

        for hit_entity in hits {
            // Skip if we already hit this entity (e.g., the locked target)
            if hit_entities.contains(&hit_entity) {
                continue;
            }

            if targets.get(hit_entity).is_ok() {
                let knockback_dir = forward.as_vec3();

                commands.trigger(DamageEvent {
                    target: hit_entity,
                    damage,
                    knockback_direction: knockback_dir,
                    knockback_force: knockback,
                    is_crit,
                });
                hit_entities.push(hit_entity);
            }
        }
    }
}

/// Observer: apply damage and knockback when DamageEvent is triggered.
fn on_damage(
    on: On<DamageEvent>,
    mut locked_target: ResMut<LockedTarget>,
    mut targets: Query<(&mut Health, Option<&mut TnuaController>)>,
    enemies: Query<(), With<Enemy>>,
    mut commands: Commands,
) {
    let event = on.event();

    let Ok((mut health, controller)) = targets.get_mut(event.target) else {
        return;
    };

    // Auto-lock on hit if not already locked and target is an enemy
    if !locked_target.is_locked() && enemies.contains(event.target) {
        locked_target.set(Some(event.target));
    }

    let died = health.take_damage(event.damage);

    // Trigger hit feedback
    commands.trigger(HitEvent {
        target: event.target,
        damage: event.damage,
        is_crit: event.is_crit,
    });

    // Apply knockback
    if let Some(mut controller) = controller {
        // Player uses Tnua
        controller.basis(TnuaBuiltinWalk {
            desired_velocity: event.knockback_direction * event.knockback_force,
            ..default()
        });
    } else {
        // Enemies use direct velocity
        let velocity = event.knockback_direction * event.knockback_force;
        commands.entity(event.target).insert(LinearVelocity(velocity));
    }

    if died {
        commands.trigger(DeathEvent {
            entity: event.target,
        });
    }
}

/// Observer: handle entity death.
fn on_death(on: On<DeathEvent>, mut commands: Commands) {
    let event = on.event();
    // For now, just despawn. Later: death animation, loot, etc.
    commands.entity(event.entity).despawn();
}

/// Apply separation force to prevent player from climbing on enemies.
/// Pushes player and enemies apart when too close.
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

    for enemy_tf in enemies.iter() {
        let enemy_pos = enemy_tf.translation;

        // Calculate horizontal distance only (ignore Y)
        let to_player = Vec3::new(
            player_pos.x - enemy_pos.x,
            0.0,
            player_pos.z - enemy_pos.z,
        );
        let distance = to_player.length();

        if distance < SEPARATION_DISTANCE && distance > 0.01 {
            // Push player away from enemy
            let push_dir = to_player.normalize();
            let push_strength = (1.0 - distance / SEPARATION_DISTANCE) * SEPARATION_FORCE;

            player_vel.x += push_dir.x * push_strength;
            player_vel.z += push_dir.z * push_strength;
        }
    }
}

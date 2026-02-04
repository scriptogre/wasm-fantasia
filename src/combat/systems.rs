use super::*;
use avian3d::spatial_query::{SpatialQuery, SpatialQueryFilter};
use bevy_enhanced_input::prelude::Start;
use bevy_tnua::prelude::*;

/// Attack configuration
pub const ATTACK_RANGE: f32 = 2.0;
pub const ATTACK_RADIUS: f32 = 1.2;
pub const ATTACK_DAMAGE: f32 = 25.0;
pub const ATTACK_KNOCKBACK: f32 = 25.0;
pub const ATTACK_HIT_FRAME: u32 = 12; // Delay hit to match visual punch connection

pub fn plugin(app: &mut App) {
    app.add_observer(handle_attack)
        .add_observer(on_damage)
        .add_observer(on_death)
        .add_systems(
            Update,
            (tick_attack_state, process_attack_hits)
                .chain()
                .run_if(in_state(Screen::Gameplay)),
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
        attack_state.start_attack();
    }
}

/// Tick attack state timers and frames.
fn tick_attack_state(time: Res<Time>, mut query: Query<&mut AttackState>) {
    for mut state in query.iter_mut() {
        state.cooldown.tick(time.delta());

        if state.attacking {
            state.attack_frame += 1;

            // Let animation play through to punch connection
            if state.attack_frame > 25 {
                state.attacking = false;
                state.attack_frame = 0;
            }
        }
    }
}

/// Check for hits on the attack's active frame using spatial query.
fn process_attack_hits(
    spatial: SpatialQuery,
    attackers: Query<(Entity, &AttackState, &Transform), With<PlayerCombatant>>,
    targets: Query<Entity, (With<Health>, With<Enemy>)>,
    mut commands: Commands,
) {
    for (attacker_entity, attack_state, transform) in attackers.iter() {
        // Only check on the hit frame
        if !attack_state.attacking || attack_state.attack_frame != ATTACK_HIT_FRAME {
            continue;
        }

        // Calculate attack position (in front of player)
        let forward = transform.forward();
        let attack_pos = transform.translation + forward * ATTACK_RANGE * 0.5;

        // Spatial query for hits
        let shape = Collider::sphere(ATTACK_RADIUS);
        let hits = spatial.shape_intersections(
            &shape,
            attack_pos,
            Quat::IDENTITY,
            &SpatialQueryFilter::default().with_excluded_entities([attacker_entity]),
        );

        for hit_entity in hits {
            // Only damage valid targets
            if targets.get(hit_entity).is_ok() {
                let knockback_dir = forward.as_vec3();

                commands.trigger(DamageEvent {
                    target: hit_entity,
                    damage: ATTACK_DAMAGE,
                    knockback_direction: knockback_dir,
                    knockback_force: ATTACK_KNOCKBACK,
                });
            }
        }
    }
}

/// Observer: apply damage and knockback when DamageEvent is triggered.
fn on_damage(
    on: On<DamageEvent>,
    mut targets: Query<(&mut Health, Option<&mut TnuaController>)>,
    mut commands: Commands,
) {
    let event = on.event();

    let Ok((mut health, controller)) = targets.get_mut(event.target) else {
        return;
    };

    let died = health.take_damage(event.damage);

    // Trigger hit feedback
    commands.trigger(HitEvent {
        target: event.target,
        damage: event.damage,
    });

    // Apply knockback via Tnua if available
    if let Some(mut controller) = controller {
        // Apply velocity directly since TnuaBuiltinKnockback API changed
        controller.basis(TnuaBuiltinWalk {
            desired_velocity: event.knockback_direction * event.knockback_force,
            ..default()
        });
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

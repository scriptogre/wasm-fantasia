use super::*;

/// Same interpolation speed as MP entity sync — knockback looks identical.
/// TODO(server-physics): Remove once knockback is physics-based.
const KNOCKBACK_LERP_SPEED: f32 = 12.0;

pub fn plugin(app: &mut App) {
    app.add_observer(on_damage)
        .add_observer(on_death)
        .add_systems(
            Update,
            drain_knockback.run_if(in_state(Screen::Gameplay)),
        );
}

/// Observer: apply damage and knockback when [`DamageDealt`] is triggered.
///
/// Server-owned entities: health AND knockback are server-authoritative —
/// the reconciler handles both. VFX still fires for immediate feedback.
///
/// The local player is the only entity whose health is mutated client-side
/// (as a prediction — reconciler overwrites it from the server each frame).
fn on_damage(
    on: On<DamageDealt>,
    mut targets: Query<(&mut Health, Option<&mut KnockbackRemaining>)>,
    #[cfg(feature = "multiplayer")] server_entities: Query<
        (),
        With<crate::networking::ServerId>,
    >,
    mut commands: Commands,
) {
    let event = on.event();

    let Ok((mut health, knockback)) = targets.get_mut(event.target) else {
        return;
    };

    // Server-owned entities: server handles health + knockback, reconciler propagates.
    // Only the local player (no ServerId) gets client-side damage applied.
    #[cfg(feature = "multiplayer")]
    let is_server_owned = server_entities.get(event.target).is_ok();
    #[cfg(not(feature = "multiplayer"))]
    let is_server_owned = false;

    let died = if is_server_owned {
        false
    } else {
        let died = health.take_damage(event.damage);
        // Queue knockback displacement — drained smoothly by drain_knockback
        if let Some(mut kb) = knockback {
            kb.0 += event.force;
        } else {
            commands
                .entity(event.target)
                .insert(KnockbackRemaining(event.force));
        }
        died
    };

    // Trigger hit feedback (VFX, damage numbers, screen shake, etc.) regardless
    commands.trigger(HitLanded {
        source: event.source,
        target: event.target,
        damage: event.damage,
        is_crit: event.is_crit,
        feedback: event.feedback.clone(),
    });

    if died {
        commands.trigger(Died {
            killer: event.source,
            entity: event.target,
        });
    }
}

/// Observer: handle entity death.
/// Server-owned entities are handled by the reconciler, not despawned locally.
fn on_death(
    on: On<Died>,
    #[cfg(feature = "multiplayer")] server_entities: Query<
        (),
        With<crate::networking::ServerId>,
    >,
    mut commands: Commands,
) {
    let event = on.event();

    #[cfg(feature = "multiplayer")]
    if server_entities.get(event.entity).is_ok() {
        return;
    }

    commands.entity(event.entity).despawn();
}

/// Smoothly apply remaining knockback displacement each frame.
/// Uses the same lerp speed as MP interpolation for visual consistency.
/// TODO(server-physics): Delete — physics impulse + engine deceleration replaces this.
fn drain_knockback(
    time: Res<Time>,
    mut query: Query<(Entity, &mut KnockbackRemaining, &mut Transform)>,
    mut commands: Commands,
) {
    let alpha = (time.delta_secs() * KNOCKBACK_LERP_SPEED).min(1.0);
    for (entity, mut kb, mut transform) in &mut query {
        let step = kb.0 * alpha;
        transform.translation += step;
        kb.0 -= step;
        if kb.0.length_squared() < 0.0001 {
            commands.entity(entity).remove::<KnockbackRemaining>();
        }
    }
}

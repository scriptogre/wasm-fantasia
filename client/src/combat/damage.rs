use super::*;
use avian3d::prelude::LinearVelocity;
use bevy_tnua::prelude::*;

pub fn plugin(app: &mut App) {
    app.add_observer(on_damage).add_observer(on_death);
}

/// Observer: apply damage and knockback when [`DamageDealt`] is triggered.
/// Server-owned entities (multiplayer): health is server-authoritative — we skip
/// `take_damage` and let the reconciler handle it. VFX and knockback still fire
/// for immediate feedback.
fn on_damage(
    on: On<DamageDealt>,
    mut targets: Query<(&mut Health, Option<&mut TnuaController>)>,
    #[cfg(feature = "multiplayer")] server_entities: Query<
        (),
        With<crate::networking::ServerId>,
    >,
    mut commands: Commands,
) {
    let event = on.event();

    let Ok((mut health, controller)) = targets.get_mut(event.target) else {
        return;
    };

    // Server-owned entities: don't modify health locally, let reconciler handle it
    #[cfg(feature = "multiplayer")]
    let is_remote = server_entities.get(event.target).is_ok();
    #[cfg(not(feature = "multiplayer"))]
    let is_remote = false;

    let died = if is_remote {
        false // Server decides death
    } else {
        health.take_damage(event.damage)
    };

    // Trigger hit feedback (VFX, damage numbers, screen shake, etc.) regardless
    commands.trigger(HitLanded {
        source: event.source,
        target: event.target,
        damage: event.damage,
        is_crit: event.is_crit,
        feedback: event.feedback.clone(),
    });

    // Apply force (knockback, launch, pull, etc.)
    if let Some(mut controller) = controller {
        controller.basis(TnuaBuiltinWalk {
            desired_velocity: event.force,
            ..default()
        });
    } else {
        commands
            .entity(event.target)
            .insert(LinearVelocity(event.force));
    }

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

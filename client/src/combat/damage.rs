use super::*;
use avian3d::prelude::LinearVelocity;
use bevy_tnua::prelude::*;

pub fn plugin(app: &mut App) {
    app.add_observer(on_damage).add_observer(on_death);
}

/// Observer: apply damage and knockback when [`DamageDealt`] is triggered.
/// For remote players (multiplayer), health is server-authoritative — we skip
/// `take_damage` and let the networking sync handle it. VFX and knockback still fire
/// for immediate feedback.
fn on_damage(
    on: On<DamageDealt>,
    mut targets: Query<(&mut Health, Option<&mut TnuaController>)>,
    #[cfg(feature = "multiplayer")] remote_players: Query<
        (),
        With<crate::networking::player::RemotePlayer>,
    >,
    #[cfg(feature = "multiplayer")] server_enemies: Query<
        (),
        With<crate::networking::combat::ServerEnemy>,
    >,
    mut commands: Commands,
) {
    let event = on.event();

    let Ok((mut health, controller)) = targets.get_mut(event.target) else {
        return;
    };

    // Server-owned entities: don't modify health locally, let sync handle it
    #[cfg(feature = "multiplayer")]
    let is_remote =
        remote_players.get(event.target).is_ok() || server_enemies.get(event.target).is_ok();
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
        // Player uses Tnua
        controller.basis(TnuaBuiltinWalk {
            desired_velocity: event.force,
            ..default()
        });
    } else {
        // Enemies use direct velocity
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
/// Remote players (multiplayer) are handled by the networking layer, not despawned locally.
fn on_death(
    on: On<Died>,
    #[cfg(feature = "multiplayer")] remote_players: Query<
        (),
        With<crate::networking::player::RemotePlayer>,
    >,
    #[cfg(feature = "multiplayer")] server_enemies: Query<
        (),
        With<crate::networking::combat::ServerEnemy>,
    >,
    mut commands: Commands,
) {
    let event = on.event();

    // Don't despawn server-owned entities — server owns their lifecycle
    #[cfg(feature = "multiplayer")]
    if remote_players.get(event.entity).is_ok() || server_enemies.get(event.entity).is_ok() {
        return;
    }

    // For now, just despawn. Later: death animation, loot, etc.
    commands.entity(event.entity).despawn();
}

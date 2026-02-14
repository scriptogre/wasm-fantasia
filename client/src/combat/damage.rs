use super::*;
use crate::player::ControlScheme;
use bevy_tnua::builtins::TnuaBuiltinKnockback;
use bevy_tnua::prelude::{TnuaController, TnuaUserControlsSystems};

/// Scale applied to the knockback vector before passing it to Tnua as a shove.
/// The knockback value from `defaults::KNOCKBACK` is already in m/s (6.0), so
/// the scale only needs to compensate for Tnua's walk basis counteracting the
/// shove. 1.0 = shove velocity equals knockback value directly.
const KNOCKBACK_SHOVE_SCALE: f32 = 1.0;

pub fn plugin(app: &mut App) {
    app.add_observer(on_damage)
        .add_observer(on_death)
        .add_systems(
            Update,
            apply_pending_knockback
                .after(TnuaUserControlsSystems)
                .run_if(in_state(Screen::Gameplay)),
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
    mut targets: Query<&mut Health>,
    server_entities: Query<(), With<crate::networking::ServerId>>,
    mut commands: Commands,
) {
    let event = on.event();

    let Ok(mut health) = targets.get_mut(event.target) else {
        return;
    };

    // Server-owned entities: server handles health + knockback, reconciler propagates.
    // Only the local player (no ServerId) gets client-side damage applied.
    let is_server_owned = server_entities.get(event.target).is_ok();

    let died = if is_server_owned {
        false
    } else {
        let died = health.take_damage(event.damage);
        // Queue knockback for the next Tnua action feeding cycle.
        // Applied by apply_pending_knockback after movement() has called
        // initiate_action_feeding(), ensuring the shove isn't cleared.
        if event.force.length_squared() > 0.0001 {
            commands
                .entity(event.target)
                .insert(PendingKnockback(event.force * KNOCKBACK_SHOVE_SCALE));
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

/// Feed the pending knockback shove into Tnua's action pipeline.
/// Runs after `TnuaUserControlsSystems` so that `initiate_action_feeding()`
/// in the movement system has already been called this frame.
fn apply_pending_knockback(
    mut query: Query<(
        Entity,
        &PendingKnockback,
        &mut TnuaController<ControlScheme>,
    )>,
    mut commands: Commands,
) {
    for (entity, knockback, mut controller) in &mut query {
        controller.action_interrupt(ControlScheme::Knockback(TnuaBuiltinKnockback {
            shove: knockback.0,
            force_forward: None,
        }));
        commands.entity(entity).remove::<PendingKnockback>();
    }
}

/// Observer: handle entity death.
/// Server-owned entities are handled by the reconciler, not despawned locally.
fn on_death(
    on: On<Died>,
    server_entities: Query<(), With<crate::networking::ServerId>>,
    mut commands: Commands,
) {
    let event = on.event();

    if server_entities.get(event.entity).is_ok() {
        return;
    }

    commands.entity(event.entity).despawn();
}

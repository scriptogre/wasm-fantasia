use super::*;
use avian3d::prelude::LinearVelocity;
use bevy_tnua::prelude::*;

pub fn plugin(app: &mut App) {
    app.add_observer(on_damage).add_observer(on_death);
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

    // Trigger hit feedback with computed values from rules
    commands.trigger(HitEvent {
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
        commands.trigger(DeathEvent {
            killer: event.source,
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

use spacetimedb::Table;
use wasm_fantasia_shared::combat::defaults;

use crate::schema::*;

#[spacetimedb::reducer]
pub fn join_game(ctx: &spacetimedb::ReducerContext, name: Option<String>, world_id: String) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    if let Some(existing) = ctx.db.player().identity().find(ctx.sender) {
        ctx.db.player().identity().update(Player {
            online: true,
            world_id,
            health: existing.max_health,
            last_update: now,
            ..existing
        });
    } else {
        ctx.db.player().insert(Player {
            identity: ctx.sender,
            name,
            online: true,
            world_id,
            x: 0.0,
            y: 1.0,
            z: 0.0,
            rotation_y: 0.0,
            animation_state: "Idle".to_string(),
            attack_sequence: 0,
            attack_animation: String::new(),
            last_update: now,
            health: defaults::HEALTH,
            max_health: defaults::HEALTH,
            attack_damage: defaults::ATTACK_DAMAGE,
            crit_chance: defaults::CRIT_CHANCE,
            crit_multiplier: defaults::CRIT_MULTIPLIER,
            attack_range: defaults::ATTACK_RANGE,
            attack_arc: defaults::ATTACK_ARC,
            knockback_force: defaults::KNOCKBACK,
            attack_speed: defaults::ATTACK_SPEED,
            last_attack_time: 0,
        });
    }
}

/// Reset health to max and reposition player at spawn point.
#[spacetimedb::reducer]
pub fn respawn(ctx: &spacetimedb::ReducerContext) {
    let Some(player) = ctx.db.player().identity().find(ctx.sender) else {
        return;
    };

    if player.health > 0.0 {
        return;
    }

    let now = ctx.timestamp.to_micros_since_unix_epoch();

    // Clear stacking buff on respawn
    let stacking: Vec<ActiveEffect> = ctx
        .db
        .active_effect()
        .iter()
        .filter(|e| e.owner == ctx.sender)
        .collect();
    for effect in stacking {
        ctx.db.active_effect().delete(effect);
    }

    ctx.db.player().identity().update(Player {
        health: player.max_health,
        x: 0.0,
        y: 1.0,
        z: 0.0,
        attack_speed: 1.0,
        last_update: now,
        ..player
    });
}

#[spacetimedb::reducer]
pub fn leave_game(ctx: &spacetimedb::ReducerContext) {
    set_player_offline(ctx);
}

/// Server-authoritative disconnect handler. Fires when the WebSocket drops,
/// regardless of whether the client managed to call leave_game().
#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &spacetimedb::ReducerContext) {
    set_player_offline(ctx);
}

fn set_player_offline(ctx: &spacetimedb::ReducerContext) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender) {
        let world_id = player.world_id.clone();

        ctx.db.player().identity().update(Player {
            online: false,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
            ..player
        });

        // Clean up solo world data to prevent abandoned state accumulating.
        // "shared" is the multiplayer world — never delete its entities.
        if world_id != "shared" {
            let enemies: Vec<Enemy> = ctx
                .db
                .enemy()
                .iter()
                .filter(|e| e.world_id == world_id)
                .collect();
            for enemy in enemies {
                ctx.db.enemy().delete(enemy);
            }
            let events: Vec<CombatEvent> = ctx
                .db
                .combat_event()
                .iter()
                .filter(|e| e.world_id == world_id)
                .collect();
            for event in events {
                ctx.db.combat_event().delete(event);
            }
        }
    }
}

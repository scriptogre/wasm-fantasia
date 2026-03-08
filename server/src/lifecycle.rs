use game_core::combat::defaults;
use spacetimedb::Table;

use crate::schema::*;

#[spacetimedb::reducer]
pub fn join_game(ctx: &spacetimedb::ReducerContext, name: Option<String>, world_id: u32) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    if let Some(existing) = ctx.db.player().identity().find(ctx.sender()) {
        ctx.db.player().identity().update(Player {
            online: true,
            world_id,
            health: existing.max_health,
            last_update: now,
            ..existing
        });
    } else {
        ctx.db.player().insert(Player {
            identity: ctx.sender(),
            name,
            online: true,
            world_id,
            x: 0.0,
            y: 1.0,
            z: 0.0,
            rotation_y: 0.0,
            animation_state: 0,
            attack_sequence: 0,
            attack_animation: 0,
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

/// Return a player reset to full health at the spawn point.
fn reset_player(player: Player, now: i64) -> Player {
    Player {
        health: player.max_health,
        x: 0.0,
        y: 1.0,
        z: 0.0,
        attack_speed: 1.0,
        last_update: now,
        ..player
    }
}

/// Clear all active effects owned by the given identity.
fn clear_effects(ctx: &spacetimedb::ReducerContext) {
    let effect_ids: Vec<u64> = ctx
        .db
        .active_effect()
        .iter()
        .filter(|e| e.owner == ctx.sender())
        .map(|e| e.id)
        .collect();
    for id in effect_ids {
        ctx.db.active_effect().id().delete(id);
    }
}

/// Reset health to max and reposition player at spawn point.
#[spacetimedb::reducer]
pub fn respawn(ctx: &spacetimedb::ReducerContext) {
    let Some(player) = ctx.db.player().identity().find(ctx.sender()) else {
        return;
    };

    if player.health > 0.0 {
        return;
    }

    let now = ctx.timestamp.to_micros_since_unix_epoch();
    clear_effects(ctx);
    ctx.db.player().identity().update(reset_player(player, now));
}

/// Full run restart: respawn the player and clear all enemies in their world.
#[spacetimedb::reducer]
pub fn restart_run(ctx: &spacetimedb::ReducerContext) {
    let Some(player) = ctx.db.player().identity().find(ctx.sender()) else {
        return;
    };

    let world_id = player.world_id;
    let is_solo = world_id != 0;
    let now = ctx.timestamp.to_micros_since_unix_epoch();

    clear_effects(ctx);
    ctx.db.player().identity().update(reset_player(player, now));

    // Only clear enemies in solo worlds — never wipe a shared multiplayer world.
    if is_solo {
        let enemy_ids: Vec<u64> = ctx
            .db
            .enemy()
            .iter()
            .filter(|e| e.world_id == world_id)
            .map(|e| e.id)
            .collect();
        for id in enemy_ids {
            ctx.db.enemy().id().delete(id);
        }
    }
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
    if let Some(player) = ctx.db.player().identity().find(ctx.sender()) {
        let world_id = player.world_id;
        let is_solo = world_id != 0;

        ctx.db.player().identity().update(Player {
            online: false,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
            ..player
        });

        // Clean up solo world data to prevent abandoned state accumulating.
        // "shared" is the multiplayer world — never delete its entities.
        if is_solo {
            let enemy_ids: Vec<u64> = ctx
                .db
                .enemy()
                .iter()
                .filter(|e| e.world_id == world_id)
                .map(|e| e.id)
                .collect();
            for id in enemy_ids {
                ctx.db.enemy().id().delete(id);
            }
            let event_ids: Vec<u64> = ctx
                .db
                .combat_event()
                .iter()
                .filter(|e| e.world_id == world_id)
                .map(|e| e.id)
                .collect();
            for id in event_ids {
                ctx.db.combat_event().id().delete(id);
            }
        }
    }
}

use spacetimedb::{Table, TimeDuration};

mod combat;
mod enemy_ai;
mod lifecycle;
pub mod schema;

pub use schema::*;

/// Server tick interval: ~33ms (30 ticks/second).
pub const TICK_INTERVAL_MICROS: i64 = 33_333;

#[spacetimedb::reducer(init)]
pub fn init(ctx: &spacetimedb::ReducerContext) {
    // Schedule repeating game tick
    ctx.db.tick_schedule().insert(TickSchedule {
        scheduled_id: 0,
        scheduled_at: TimeDuration::from_micros(TICK_INTERVAL_MICROS).into(),
    });
    spacetimedb::log::info!(
        "Server initialized — game tick scheduled at {}ms interval",
        TICK_INTERVAL_MICROS / 1000
    );
}

#[spacetimedb::reducer]
pub fn pause_world(ctx: &spacetimedb::ReducerContext) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender) {
        let _ = ctx.db.world_pause().insert(WorldPause {
            world_id: player.world_id.clone(),
        });
    }
}

#[spacetimedb::reducer]
pub fn resume_world(ctx: &spacetimedb::ReducerContext) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender) {
        ctx.db.world_pause().world_id().delete(&player.world_id);
    }
}

/// Client state relay.
#[spacetimedb::reducer]
pub fn update_position(
    ctx: &spacetimedb::ReducerContext,
    x: f32,
    y: f32,
    z: f32,
    rotation_y: f32,
    animation_state: String,
    attack_sequence: u32,
    attack_animation: String,
) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender) {
        ctx.db.player().identity().update(Player {
            x,
            y,
            z,
            rotation_y,
            animation_state,
            attack_sequence,
            attack_animation,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
            ..player
        });
    }
}

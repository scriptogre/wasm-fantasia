use spacetimedb::Table;

#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: spacetimedb::Identity,
    pub name: Option<String>,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rot_y: f32,
    pub anim_state: String,
    pub online: bool,
    pub last_update: i64,
}

#[spacetimedb::reducer]
pub fn join_game(ctx: &spacetimedb::ReducerContext, name: Option<String>) {
    // Check if player already exists (reconnecting)
    if let Some(existing) = ctx.db.player().identity().find(&ctx.sender) {
        ctx.db.player().identity().update(Player {
            online: true,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
            ..existing
        });
    } else {
        ctx.db.player().insert(Player {
            identity: ctx.sender,
            name,
            x: 0.0,
            y: 1.0,
            z: 0.0,
            rot_y: 0.0,
            anim_state: "Idle".to_string(),
            online: true,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    }
}

#[spacetimedb::reducer]
pub fn update_position(ctx: &spacetimedb::ReducerContext, x: f32, y: f32, z: f32, rot_y: f32, anim_state: String) {
    if let Some(player) = ctx.db.player().identity().find(&ctx.sender) {
        ctx.db.player().identity().update(Player {
            x,
            y,
            z,
            rot_y,
            anim_state,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
            ..player
        });
    }
}

#[spacetimedb::reducer]
pub fn leave_game(ctx: &spacetimedb::ReducerContext) {
    if let Some(player) = ctx.db.player().identity().find(&ctx.sender) {
        ctx.db.player().identity().update(Player {
            online: false,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
            ..player
        });
    }
}

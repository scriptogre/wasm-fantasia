use spacetimedb::Table;

/// Player state stored on the server (authoritative)
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: spacetimedb::Identity,
    pub name: Option<String>,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rot_y: f32,
    pub vel_x: f32,
    pub vel_z: f32,
    pub on_ground: bool,
    pub anim_state: String,
    pub online: bool,
    pub last_update: i64,
}

/// Client input state - authoritative source of what the player is trying to do
#[spacetimedb::table(name = player_input, public)]
#[derive(Clone)]
pub struct PlayerInput {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub identity: spacetimedb::Identity,
    pub sequence: u32,  // Incrementing sequence number for input
    pub forward: f32,   // -1.0 to 1.0 (W/S or up/down)
    pub right: f32,     // -1.0 to 1.0 (A/D or left/right)
    pub jump: bool,
    pub sprint: bool,
    pub crouch: bool,
    pub yaw: f32,       // Camera yaw for movement direction
    pub timestamp: i64,
}

/// Movement constants (should match client-side Tnua config)
const MOVE_SPEED: f32 = 6.0;
const SPRINT_MULTIPLIER: f32 = 1.6;
const CROUCH_MULTIPLIER: f32 = 0.3;
const GRAVITY: f32 = 20.0;
const DT: f32 = 1.0 / 60.0;  // 60 Hz tick rate

#[spacetimedb::reducer]
pub fn join_game(ctx: &spacetimedb::ReducerContext, name: Option<String>) {
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
            vel_x: 0.0,
            vel_z: 0.0,
            on_ground: true,
            anim_state: "Idle".to_string(),
            online: true,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    }
}

#[spacetimedb::reducer]
pub fn send_input(
    ctx: &spacetimedb::ReducerContext,
    sequence: u32,
    forward: f32,
    right: f32,
    jump: bool,
    sprint: bool,
    crouch: bool,
    yaw: f32,
) {
    // Store the input - will be processed by next tick
    ctx.db.player_input().insert(PlayerInput {
        id: 0, // Auto-incremented
        identity: ctx.sender,
        sequence,
        forward,
        right,
        jump,
        sprint,
        crouch,
        yaw,
        timestamp: ctx.timestamp.to_micros_since_unix_epoch(),
    });
}

#[spacetimedb::reducer]
pub fn game_tick(ctx: &spacetimedb::ReducerContext) {
    // Process all pending inputs and simulate movement
    for input in ctx.db.player_input().iter() {
        if let Some(mut player) = ctx.db.player().identity().find(&input.identity) {
            // Calculate movement speed
            let mut speed = MOVE_SPEED;
            if input.sprint {
                speed *= SPRINT_MULTIPLIER;
            }
            if input.crouch {
                speed *= CROUCH_MULTIPLIER;
            }

            // Calculate movement direction based on camera yaw
            let yaw_sin = input.yaw.sin();
            let yaw_cos = input.yaw.cos();

            // Forward/backward movement
            let move_x = input.forward * yaw_sin + input.right * yaw_cos;
            let move_z = input.forward * yaw_cos - input.right * yaw_sin;

            // Update horizontal velocity (simplified - no inertia for now)
            player.vel_x = move_x * speed;
            player.vel_z = move_z * speed;

            // Apply gravity
            if !player.on_ground {
                player.y -= GRAVITY * DT;
            }

            // Jump
            if input.jump && player.on_ground {
                player.y += 0.1; // Small hop off ground
                player.on_ground = false;
            }

            // Ground check (simplified flat ground at y=0)
            if player.y <= 1.0 {
                player.y = 1.0;
                player.on_ground = true;
            }

            // Apply velocity
            player.x += player.vel_x * DT;
            player.z += player.vel_z * DT;

            // Determine animation state
            player.anim_state = if !player.on_ground {
                "Jump".to_string()
            } else if input.crouch {
                "Crouch".to_string()
            } else if input.sprint && (input.forward.abs() > 0.1 || input.right.abs() > 0.1) {
                "Run".to_string()
            } else if input.forward.abs() > 0.1 || input.right.abs() > 0.1 {
                "Walk".to_string()
            } else {
                "Idle".to_string()
            };

            // Update rotation to face movement direction
            if input.forward.abs() > 0.01 || input.right.abs() > 0.01 {
                player.rot_y = yaw_sin.atan2(input.forward * yaw_cos - input.right * yaw_sin);
            }

            player.last_update = ctx.timestamp.to_micros_since_unix_epoch();
            ctx.db.player().identity().update(player);
        }

        // Remove processed input
        ctx.db.player_input().delete(input);
    }
}

/// Simple position relay — client sends its position directly.
/// Temporary until input-based model is wired end-to-end.
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

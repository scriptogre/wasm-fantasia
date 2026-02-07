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
    pub attack_seq: u32,
    pub attack_anim: String,
    pub online: bool,
    pub last_update: i64,

    // Combat - health
    pub health: f32,
    pub max_health: f32,

    // Combat - offensive stats
    pub attack_damage: f32,
    pub crit_chance: f32,
    pub crit_multiplier: f32,
    pub attack_range: f32,
    pub attack_arc: f32,
    pub knockback_force: f32,
    pub attack_speed: f32,

    // Combat - stacking state
    pub stacks: f32,
    pub stack_decay: f32,
    pub last_hit_time: i64,

    // Combat - cooldown
    pub last_attack_time: i64,
}

/// Ephemeral hit result. Inserted by attack_hit, consumed by clients for VFX.
/// All clients see these via subscription and trigger damage numbers, flash, etc.
#[spacetimedb::table(name = combat_event, public)]
pub struct CombatEvent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub attacker: spacetimedb::Identity,
    pub target_player: Option<spacetimedb::Identity>,
    pub target_npc_id: Option<u64>,
    pub damage: f32,
    pub is_crit: bool,
    pub attacker_x: f32,
    pub attacker_z: f32,
    pub timestamp: i64,
}

/// Server-authoritative NPC enemy. All clients see and can attack these.
#[spacetimedb::table(name = npc_enemy, public)]
pub struct NpcEnemy {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub health: f32,
    pub max_health: f32,
}

/// Default combat stats for new players
mod defaults {
    pub const HEALTH: f32 = 100.0;
    pub const ATTACK_DAMAGE: f32 = 25.0;
    pub const CRIT_CHANCE: f32 = 0.2;
    pub const CRIT_MULTIPLIER: f32 = 2.5;
    pub const ATTACK_RANGE: f32 = 3.6;
    pub const ATTACK_ARC: f32 = 150.0;
    pub const KNOCKBACK_FORCE: f32 = 3.0;
    pub const ATTACK_SPEED: f32 = 1.0;
    pub const STACK_DECAY: f32 = 2.5;
    /// Base attack cooldown in microseconds (0.42s)
    pub const ATTACK_COOLDOWN_MICROS: i64 = 420_000;
    pub const ENEMY_HEALTH: f32 = 500.0;
}

/// Client input state - authoritative source of what the player is trying to do
#[spacetimedb::table(name = player_input, public)]
#[derive(Clone)]
pub struct PlayerInput {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub identity: spacetimedb::Identity,
    pub sequence: u32,
    pub forward: f32,
    pub right: f32,
    pub jump: bool,
    pub sprint: bool,
    pub crouch: bool,
    pub yaw: f32,
    pub timestamp: i64,
}

/// Movement constants (should match client-side Tnua config)
const MOVE_SPEED: f32 = 6.0;
const SPRINT_MULTIPLIER: f32 = 1.6;
const CROUCH_MULTIPLIER: f32 = 0.3;
const GRAVITY: f32 = 20.0;
const DT: f32 = 1.0 / 60.0;

/// Deterministic pseudo-random from timestamp and identity bytes.
/// Returns a value in [0.0, 1.0).
fn deterministic_random(timestamp_micros: i64, identity: &spacetimedb::Identity) -> f32 {
    let id_bytes = identity.to_byte_array();
    let mut hash: u64 = timestamp_micros as u64;
    for &b in &id_bytes[..8] {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

/// Deterministic pseudo-random from timestamp and a u64 seed.
fn deterministic_random_u64(timestamp_micros: i64, seed: u64) -> f32 {
    let mut hash: u64 = timestamp_micros as u64;
    hash ^= seed;
    hash = hash.wrapping_mul(0x100000001b3);
    hash ^= seed >> 32;
    hash = hash.wrapping_mul(0x100000001b3);
    (hash & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

#[spacetimedb::reducer]
pub fn join_game(ctx: &spacetimedb::ReducerContext, name: Option<String>) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    if let Some(existing) = ctx.db.player().identity().find(&ctx.sender) {
        ctx.db.player().identity().update(Player {
            online: true,
            health: existing.max_health,
            last_update: now,
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
            attack_seq: 0,
            attack_anim: String::new(),
            online: true,
            last_update: now,
            health: defaults::HEALTH,
            max_health: defaults::HEALTH,
            attack_damage: defaults::ATTACK_DAMAGE,
            crit_chance: defaults::CRIT_CHANCE,
            crit_multiplier: defaults::CRIT_MULTIPLIER,
            attack_range: defaults::ATTACK_RANGE,
            attack_arc: defaults::ATTACK_ARC,
            knockback_force: defaults::KNOCKBACK_FORCE,
            attack_speed: defaults::ATTACK_SPEED,
            stacks: 0.0,
            stack_decay: defaults::STACK_DECAY,
            last_hit_time: 0,
            last_attack_time: 0,
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
    ctx.db.player_input().insert(PlayerInput {
        id: 0,
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
    for input in ctx.db.player_input().iter() {
        if let Some(mut player) = ctx.db.player().identity().find(&input.identity) {
            let mut speed = MOVE_SPEED;
            if input.sprint {
                speed *= SPRINT_MULTIPLIER;
            }
            if input.crouch {
                speed *= CROUCH_MULTIPLIER;
            }

            let yaw_sin = input.yaw.sin();
            let yaw_cos = input.yaw.cos();

            let move_x = input.forward * yaw_sin + input.right * yaw_cos;
            let move_z = input.forward * yaw_cos - input.right * yaw_sin;

            player.vel_x = move_x * speed;
            player.vel_z = move_z * speed;

            if !player.on_ground {
                player.y -= GRAVITY * DT;
            }

            if input.jump && player.on_ground {
                player.y += 0.1;
                player.on_ground = false;
            }

            if player.y <= 1.0 {
                player.y = 1.0;
                player.on_ground = true;
            }

            player.x += player.vel_x * DT;
            player.z += player.vel_z * DT;

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

            if input.forward.abs() > 0.01 || input.right.abs() > 0.01 {
                player.rot_y = yaw_sin.atan2(input.forward * yaw_cos - input.right * yaw_sin);
            }

            player.last_update = ctx.timestamp.to_micros_since_unix_epoch();
            ctx.db.player().identity().update(player);
        }

        ctx.db.player_input().delete(input);
    }
}

/// Client state relay — client sends its visual state (position, animation) directly.
#[spacetimedb::reducer]
pub fn update_position(ctx: &spacetimedb::ReducerContext, x: f32, y: f32, z: f32, rot_y: f32, anim_state: String, attack_seq: u32, attack_anim: String) {
    if let Some(player) = ctx.db.player().identity().find(&ctx.sender) {
        ctx.db.player().identity().update(Player {
            x,
            y,
            z,
            rot_y,
            anim_state,
            attack_seq,
            attack_anim,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
            ..player
        });
    }
}

/// Server-authoritative attack resolution.
/// Checks hits against both other players and NPC enemies.
#[spacetimedb::reducer]
pub fn attack_hit(ctx: &spacetimedb::ReducerContext) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    let Some(attacker) = ctx.db.player().identity().find(&ctx.sender) else {
        return;
    };

    if attacker.health <= 0.0 {
        return;
    }

    // Cleanup old combat events (older than 5 seconds)
    let stale_threshold = now - 5_000_000;
    let stale_events: Vec<CombatEvent> = ctx
        .db
        .combat_event()
        .iter()
        .filter(|e| e.timestamp < stale_threshold)
        .collect();
    for event in stale_events {
        ctx.db.combat_event().delete(event);
    }

    let cooldown_micros = (defaults::ATTACK_COOLDOWN_MICROS as f64 / attacker.attack_speed as f64) as i64;
    if now - attacker.last_attack_time < cooldown_micros {
        return;
    }

    // Lazy stacking decay
    let decay_elapsed = (now - attacker.last_hit_time) as f64 / 1_000_000.0;
    let mut stacks = attacker.stacks;
    let mut attack_speed = attacker.attack_speed;
    if attacker.last_hit_time > 0 && decay_elapsed > attacker.stack_decay as f64 && stacks > 0.0 {
        stacks = 0.0;
        attack_speed = 1.0;
    }

    let half_arc_cos = (attacker.attack_arc / 2.0_f32).to_radians().cos();
    // Bevy's forward is -local_z: for Quat::from_rotation_y(rot_y), forward = (-sin(rot_y), -cos(rot_y))
    let fwd_x = -attacker.rot_y.sin();
    let fwd_z = -attacker.rot_y.cos();

    let mut hit_someone = false;

    // --- Hit detection against other players ---
    let player_targets: Vec<Player> = ctx
        .db
        .player()
        .iter()
        .filter(|t| t.identity != ctx.sender && t.online && t.health > 0.0)
        .collect();

    for target in player_targets {
        let dx = target.x - attacker.x;
        let dz = target.z - attacker.z;
        let dist = (dx * dx + dz * dz).sqrt();

        if dist > attacker.attack_range {
            continue;
        }

        if dist > 0.01 {
            let dot = (fwd_x * dx + fwd_z * dz) / dist;
            if dot < half_arc_cos {
                continue;
            }
        }

        let roll = deterministic_random(now, &target.identity);
        let is_crit = roll < attacker.crit_chance;
        let mut damage = attacker.attack_damage;
        if is_crit {
            damage *= attacker.crit_multiplier;
        }

        let new_health = (target.health - damage).max(0.0);
        let target_identity = target.identity;

        ctx.db.player().identity().update(Player {
            health: new_health,
            last_update: now,
            ..target
        });

        ctx.db.combat_event().insert(CombatEvent {
            id: 0,
            attacker: ctx.sender,
            target_player: Some(target_identity),
            target_npc_id: None,
            damage,
            is_crit,
            attacker_x: attacker.x,
            attacker_z: attacker.z,
            timestamp: now,
        });

        hit_someone = true;
    }

    // --- Hit detection against NPC enemies ---
    let enemy_targets: Vec<NpcEnemy> = ctx
        .db
        .npc_enemy()
        .iter()
        .filter(|e| e.health > 0.0)
        .collect();

    for enemy in enemy_targets {
        let dx = enemy.x - attacker.x;
        let dz = enemy.z - attacker.z;
        let dist = (dx * dx + dz * dz).sqrt();

        if dist > attacker.attack_range {
            continue;
        }

        if dist > 0.01 {
            let dot = (fwd_x * dx + fwd_z * dz) / dist;
            if dot < half_arc_cos {
                continue;
            }
        }

        let roll = deterministic_random_u64(now, enemy.id);
        let is_crit = roll < attacker.crit_chance;
        let mut damage = attacker.attack_damage;
        if is_crit {
            damage *= attacker.crit_multiplier;
        }

        let new_health = (enemy.health - damage).max(0.0);

        ctx.db.combat_event().insert(CombatEvent {
            id: 0,
            attacker: ctx.sender,
            target_player: None,
            target_npc_id: Some(enemy.id),
            damage,
            is_crit,
            attacker_x: attacker.x,
            attacker_z: attacker.z,
            timestamp: now,
        });

        if new_health <= 0.0 {
            ctx.db.npc_enemy().delete(enemy);
        } else {
            ctx.db.npc_enemy().id().update(NpcEnemy {
                health: new_health,
                ..enemy
            });
        }

        hit_someone = true;
    }

    // Update attacker stacking state
    if hit_someone {
        stacks = (stacks + 1.0).min(12.0);
        attack_speed = 1.0 + stacks * 0.12;
    }

    ctx.db.player().identity().update(Player {
        last_attack_time: now,
        last_hit_time: if hit_someone { now } else { attacker.last_hit_time },
        stacks,
        attack_speed,
        last_update: now,
        ..attacker
    });
}

/// Spawn a pack of enemies at the given position and facing direction.
/// Called by clients when pressing E.
#[spacetimedb::reducer]
pub fn spawn_enemies(ctx: &spacetimedb::ReducerContext, x: f32, y: f32, z: f32, forward_x: f32, forward_z: f32) {
    // Verify the caller is a connected player
    let Some(_player) = ctx.db.player().identity().find(&ctx.sender) else {
        return;
    };

    // Base position: 5 units in front of caller
    let base_x = x + forward_x * 5.0;
    let base_z = z + forward_z * 5.0;

    // Right vector (perpendicular to forward in XZ plane)
    let right_x = -forward_z;
    let right_z = forward_x;

    // Formation offsets: center, right, left, far right, far left
    let offsets: [(f32, f32); 5] = [
        (0.0, 0.0),
        (1.5 * right_x - 0.5 * forward_x, 1.5 * right_z - 0.5 * forward_z),
        (-1.5 * right_x - 0.5 * forward_x, -1.5 * right_z - 0.5 * forward_z),
        (2.5 * right_x - 1.5 * forward_x, 2.5 * right_z - 1.5 * forward_z),
        (-2.5 * right_x - 1.5 * forward_x, -2.5 * right_z - 1.5 * forward_z),
    ];

    for (ox, oz) in offsets {
        ctx.db.npc_enemy().insert(NpcEnemy {
            id: 0, // auto_inc
            x: base_x + ox,
            y,
            z: base_z + oz,
            health: defaults::ENEMY_HEALTH,
            max_health: defaults::ENEMY_HEALTH,
        });
    }
}

/// Reset health to max and reposition player at spawn point.
#[spacetimedb::reducer]
pub fn respawn(ctx: &spacetimedb::ReducerContext) {
    let Some(player) = ctx.db.player().identity().find(&ctx.sender) else {
        return;
    };

    if player.health > 0.0 {
        return;
    }

    let now = ctx.timestamp.to_micros_since_unix_epoch();
    ctx.db.player().identity().update(Player {
        health: player.max_health,
        x: 0.0,
        y: 1.0,
        z: 0.0,
        stacks: 0.0,
        attack_speed: 1.0,
        last_update: now,
        ..player
    });
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

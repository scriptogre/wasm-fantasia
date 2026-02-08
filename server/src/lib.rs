use spacetimedb::Table;
use wasm_fantasia_shared::combat::{self, defaults, resolve_combat, CombatInput, HitTarget};
use wasm_fantasia_shared::presets;
use wasm_fantasia_shared::rules::{Stat, Stats};

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

/// Server-authoritative NPC enemy.
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

/// Client input state
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

#[spacetimedb::reducer]
pub fn join_game(ctx: &spacetimedb::ReducerContext, name: Option<String>) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    if let Some(existing) = ctx.db.player().identity().find(ctx.sender) {
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
            knockback_force: defaults::KNOCKBACK,
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
        if let Some(mut player) = ctx.db.player().identity().find(input.identity) {
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

/// Client state relay
#[spacetimedb::reducer]
pub fn update_position(
    ctx: &spacetimedb::ReducerContext,
    x: f32,
    y: f32,
    z: f32,
    rot_y: f32,
    anim_state: String,
    attack_seq: u32,
    attack_anim: String,
) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender) {
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
#[spacetimedb::reducer]
pub fn attack_hit(ctx: &spacetimedb::ReducerContext) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    let Some(attacker) = ctx.db.player().identity().find(ctx.sender) else {
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

    // Cooldown check using shared function
    if !combat::can_attack(attacker.last_attack_time, now, attacker.attack_speed) {
        return;
    }

    // Lazy stacking decay (server uses timestamp check, not frame-by-frame tick)
    let decay_elapsed = (now - attacker.last_hit_time) as f64 / 1_000_000.0;
    let decayed_stacks =
        combat::decay_stacks(attacker.stacks, decay_elapsed, attacker.stack_decay);
    let decayed_speed = if decayed_stacks == 0.0 && attacker.stacks > 0.0 {
        1.0
    } else {
        attacker.attack_speed
    };

    let rules = presets::default_player_rules();

    let half_arc_cos = (attacker.attack_arc / 2.0_f32).to_radians().cos();
    // Bevy's forward is -local_z: for Quat::from_rotation_y(rot_y), forward = (-sin(rot_y), -cos(rot_y))
    let fwd = glam::Vec2::new(-attacker.rot_y.sin(), -attacker.rot_y.cos());
    let origin = glam::Vec2::new(attacker.x, attacker.z);

    let attacker_stats = Stats::new()
        .with(Stat::AttackDamage, attacker.attack_damage)
        .with(Stat::CritChance, attacker.crit_chance)
        .with(Stat::CritMultiplier, attacker.crit_multiplier)
        .with(Stat::Knockback, attacker.knockback_force)
        .with(Stat::AttackRange, attacker.attack_range)
        .with(Stat::AttackArc, attacker.attack_arc)
        .with(Stat::Custom("Stacks".into()), decayed_stacks)
        .with(Stat::AttackSpeed, decayed_speed);

    // Build target list from NPC enemies
    let enemy_targets: Vec<NpcEnemy> = ctx
        .db
        .npc_enemy()
        .iter()
        .filter(|e| e.health > 0.0)
        .collect();

    let hit_targets: Vec<HitTarget> = enemy_targets
        .iter()
        .map(|e| HitTarget {
            id: e.id,
            pos: glam::Vec2::new(e.x, e.z),
            health: e.health,
        })
        .collect();

    let output = resolve_combat(&CombatInput {
        origin,
        forward: fwd,
        base_range: attacker.attack_range,
        half_arc_cos,
        attacker_stats: &attacker_stats,
        rules: &rules,
        rng_seed: now as u64,
        targets: &hit_targets,
    });

    // Apply results to DB
    for hit in &output.hits {
        ctx.db.combat_event().insert(CombatEvent {
            id: 0,
            attacker: ctx.sender,
            target_player: None,
            target_npc_id: Some(hit.target_id),
            damage: hit.damage,
            is_crit: hit.is_crit,
            attacker_x: attacker.x,
            attacker_z: attacker.z,
            timestamp: now,
        });

        // Look up current enemy state from DB (not the Vec — avoids borrow issues)
        if let Some(enemy) = ctx.db.npc_enemy().id().find(hit.target_id) {
            if hit.died {
                ctx.db.npc_enemy().delete(enemy);
            } else {
                ctx.db.npc_enemy().id().update(NpcEnemy {
                    health: hit.new_health,
                    ..enemy
                });
            }
        }
    }

    ctx.db.player().identity().update(Player {
        last_attack_time: now,
        last_hit_time: if output.hit_any {
            now
        } else {
            attacker.last_hit_time
        },
        stacks: output.attacker_stats.get(&Stat::Custom("Stacks".into())),
        attack_speed: output.attacker_stats.get(&Stat::AttackSpeed),
        last_update: now,
        ..attacker
    });
}

/// Spawn a pack of enemies at the given position and facing direction.
#[spacetimedb::reducer]
pub fn spawn_enemies(
    ctx: &spacetimedb::ReducerContext,
    x: f32,
    y: f32,
    z: f32,
    forward_x: f32,
    forward_z: f32,
) {
    let Some(_player) = ctx.db.player().identity().find(ctx.sender) else {
        return;
    };

    let base_x = x + forward_x * 5.0;
    let base_z = z + forward_z * 5.0;

    let right_x = -forward_z;
    let right_z = forward_x;

    let offsets: [(f32, f32); 5] = [
        (0.0, 0.0),
        (
            1.5 * right_x - 0.5 * forward_x,
            1.5 * right_z - 0.5 * forward_z,
        ),
        (
            -1.5 * right_x - 0.5 * forward_x,
            -1.5 * right_z - 0.5 * forward_z,
        ),
        (
            2.5 * right_x - 1.5 * forward_x,
            2.5 * right_z - 1.5 * forward_z,
        ),
        (
            -2.5 * right_x - 1.5 * forward_x,
            -2.5 * right_z - 1.5 * forward_z,
        ),
    ];

    for (ox, oz) in offsets {
        ctx.db.npc_enemy().insert(NpcEnemy {
            id: 0,
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
    let Some(player) = ctx.db.player().identity().find(ctx.sender) else {
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
        ctx.db.player().identity().update(Player {
            online: false,
            last_update: ctx.timestamp.to_micros_since_unix_epoch(),
            ..player
        });
    }
}

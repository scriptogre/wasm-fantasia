use spacetimedb::{Table, TimeDuration};
use std::collections::HashMap;
use wasm_fantasia_shared::combat::{
    self, defaults, enemy_ai_decision, resolve_combat, CombatInput, HitTarget,
};
use wasm_fantasia_shared::presets;
use wasm_fantasia_shared::rules::{Stat, Stats};

/// Player state stored on the server (authoritative).
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: spacetimedb::Identity,
    pub name: Option<String>,
    pub online: bool,
    pub world_id: String,
    pub last_update: i64,

    // Position
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,

    // Animation
    pub animation_state: String,
    pub attack_sequence: u32,
    pub attack_animation: String,

    // Health
    pub health: f32,
    pub max_health: f32,

    // Combat
    pub attack_damage: f32,
    pub crit_chance: f32,
    pub crit_multiplier: f32,
    pub attack_range: f32,
    pub attack_arc: f32,
    pub knockback_force: f32,
    pub attack_speed: f32,
    pub last_attack_time: i64,
}

/// Server-authoritative enemy.
#[spacetimedb::table(name = enemy, public)]
pub struct Enemy {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub enemy_type: String,
    pub world_id: String,

    // Position
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,

    // Animation
    pub animation_state: String,

    // Health
    pub health: f32,
    pub max_health: f32,

    // Combat
    pub attack_damage: f32,
    pub attack_range: f32,
    pub attack_speed: f32,
    pub last_attack_time: i64,
}

/// Ephemeral hit notification. Inserted by attack_hit, consumed by clients for VFX.
#[spacetimedb::table(name = combat_event, public)]
pub struct CombatEvent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub damage: f32,
    pub is_crit: bool,
    pub world_id: String,
    pub timestamp: i64,
}

/// Dynamic effect (buff, debuff, DoT). Managed by combat reducers now,
/// by Rhai/Lua scripts later.
#[spacetimedb::table(name = active_effect, public)]
pub struct ActiveEffect {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner: spacetimedb::Identity,
    pub effect_type: String,
    pub magnitude: f32,
    pub duration: f32,
    pub timestamp: i64,
}

/// Scheduled tick for server-side game logic (enemy AI, etc.).
#[spacetimedb::table(name = tick_schedule, scheduled(game_tick))]
pub struct TickSchedule {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: spacetimedb::ScheduleAt,
}

/// Server tick interval: 100ms (10 ticks/second).
const TICK_INTERVAL_MICROS: i64 = 100_000;

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

    // Cleanup old combat events in this world (older than 5 seconds)
    let stale_threshold = now - 5_000_000;
    let stale_events: Vec<CombatEvent> = ctx
        .db
        .combat_event()
        .iter()
        .filter(|e| e.world_id == attacker.world_id && e.timestamp < stale_threshold)
        .collect();
    for event in stale_events {
        ctx.db.combat_event().delete(event);
    }

    // Cooldown check
    if !combat::can_attack(attacker.last_attack_time, now, attacker.attack_speed) {
        return;
    }

    // Read stacking buff from active_effect table
    let stacking_effect = ctx
        .db
        .active_effect()
        .iter()
        .find(|e| e.owner == ctx.sender && e.effect_type == "stacking_damage");

    let (stacks, last_hit_time) = if let Some(ref effect) = stacking_effect {
        let decay_elapsed = (now - effect.timestamp) as f64 / 1_000_000.0;
        let decayed = combat::decay_stacks(effect.magnitude, decay_elapsed, defaults::STACK_DECAY);
        (decayed, effect.timestamp)
    } else {
        (0.0, 0_i64)
    };

    let effective_speed = if stacks > 0.0 {
        attacker.attack_speed
    } else {
        1.0
    };

    let rules = presets::default_player_rules();

    let half_arc_cos = (attacker.attack_arc / 2.0_f32).to_radians().cos();
    let fwd = glam::Vec2::new(-attacker.rotation_y.sin(), -attacker.rotation_y.cos());
    let origin = glam::Vec2::new(attacker.x, attacker.z);

    let attacker_stats = Stats::new()
        .with(Stat::AttackDamage, attacker.attack_damage)
        .with(Stat::CritChance, attacker.crit_chance)
        .with(Stat::CritMultiplier, attacker.crit_multiplier)
        .with(Stat::Knockback, attacker.knockback_force)
        .with(Stat::AttackRange, attacker.attack_range)
        .with(Stat::AttackArc, attacker.attack_arc)
        .with(Stat::Custom("Stacks".into()), stacks)
        .with(Stat::AttackSpeed, effective_speed);

    // Build target list from enemies in the same world
    let enemy_targets: Vec<Enemy> = ctx
        .db
        .enemy()
        .iter()
        .filter(|e| e.health > 0.0 && e.world_id == attacker.world_id)
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
        // Combat event at the target's position for VFX
        let target_enemy = enemy_targets.iter().find(|e| e.id == hit.target_id);
        let (hit_x, hit_y, hit_z) = target_enemy
            .map(|e| (e.x, e.y, e.z))
            .unwrap_or((attacker.x, attacker.y, attacker.z));

        ctx.db.combat_event().insert(CombatEvent {
            id: 0,
            x: hit_x,
            y: hit_y,
            z: hit_z,
            damage: hit.damage,
            is_crit: hit.is_crit,
            world_id: attacker.world_id.clone(),
            timestamp: now,
        });

        if let Some(enemy) = ctx.db.enemy().id().find(hit.target_id) {
            if hit.died {
                ctx.db.enemy().delete(enemy);
            } else {
                // TODO(server-physics): Replace with physics impulse once Avian3d
                // runs server-side. The engine will handle displacement natively.
                let radial = glam::Vec2::new(enemy.x - attacker.x, enemy.z - attacker.z);
                let radial_dir = radial.normalize_or(fwd);
                let disp = combat::knockback_displacement(
                    radial_dir,
                    fwd,
                    hit.knockback,
                    hit.push,
                    hit.launch,
                );

                ctx.db.enemy().id().update(Enemy {
                    health: hit.new_health,
                    x: enemy.x + disp.x,
                    z: enemy.z + disp.z,
                    ..enemy
                });
            }
        }
    }

    // Update attacker state
    let new_stacks = output.attacker_stats.get(&Stat::Custom("Stacks".into()));
    let new_speed = output.attacker_stats.get(&Stat::AttackSpeed);

    // Persist stacking buff to active_effect
    if new_stacks > 0.0 || stacking_effect.is_some() {
        if let Some(effect) = stacking_effect {
            if new_stacks > 0.0 {
                ctx.db.active_effect().id().update(ActiveEffect {
                    magnitude: new_stacks,
                    timestamp: if output.hit_any { now } else { last_hit_time },
                    ..effect
                });
            } else {
                ctx.db.active_effect().delete(effect);
            }
        } else if new_stacks > 0.0 {
            ctx.db.active_effect().insert(ActiveEffect {
                id: 0,
                owner: ctx.sender,
                effect_type: "stacking_damage".to_string(),
                magnitude: new_stacks,
                duration: -1.0,
                timestamp: now,
            });
        }
    }

    ctx.db.player().identity().update(Player {
        last_attack_time: now,
        attack_speed: new_speed,
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
    _forward_x: f32,
    _forward_z: f32,
) {
    let Some(player) = ctx.db.player().identity().find(ctx.sender) else {
        return;
    };

    let world_id = player.world_id;

    // TODO(server-abstraction): spawn logic is duplicated in client's spawn_enemy_in_front.
    // Per-enemy scatter using hash that varies meaningfully per index
    let seed = ctx.timestamp.to_micros_since_unix_epoch() as u64;
    let count = 80 + (seed % 41) as u32; // 80–120 enemies

    for i in 0..count {
        let h = (seed ^ 0xDEADBEEF)
            .wrapping_add(i as u64)
            .wrapping_mul(6364136223846793005);
        let angle = (h & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
        let radius = defaults::ENEMY_SPAWN_RADIUS_MIN
            + ((h >> 16) & 0xFFFF) as f32 / 65535.0
                * (defaults::ENEMY_SPAWN_RADIUS_MAX - defaults::ENEMY_SPAWN_RADIUS_MIN);

        ctx.db.enemy().insert(Enemy {
            id: 0,
            enemy_type: "basic".to_string(),
            world_id: world_id.clone(),
            x: x + angle.cos() * radius,
            y,
            z: z + angle.sin() * radius,
            rotation_y: 0.0,
            animation_state: "Idle".to_string(),
            health: defaults::ENEMY_HEALTH,
            max_health: defaults::ENEMY_HEALTH,
            attack_damage: defaults::ENEMY_ATTACK_DAMAGE,
            attack_range: defaults::ENEMY_ATTACK_RANGE,
            attack_speed: 1.0,
            last_attack_time: 0,
        });
    }
}

// =============================================================================
// Server-side enemy AI tick
// =============================================================================
//
// TODO(server-abstraction): This reducer duplicates the movement + facing logic
// that also lives in the client's `enemy_ai` system (combat/enemy.rs). When the
// SP/MP backend trait lands, both code paths collapse into a single
// `GameServer::tick_enemies` implementation. The shared decision function
// `enemy_ai_decision()` (shared/src/combat.rs) already centralises the
// state-machine; what remains duplicated is the movement application and facing.

/// Periodic server tick — drives enemy AI for multiplayer.
#[spacetimedb::reducer]
pub fn game_tick(ctx: &spacetimedb::ReducerContext, _args: TickSchedule) {
    let dt = TICK_INTERVAL_MICROS as f32 / 1_000_000.0;
    let now = ctx.timestamp.to_micros_since_unix_epoch();

    // Group alive online players by world_id
    let mut players_by_world: HashMap<String, Vec<Player>> = HashMap::new();
    for p in ctx
        .db
        .player()
        .iter()
        .filter(|p| p.online && p.health > 0.0)
    {
        players_by_world
            .entry(p.world_id.clone())
            .or_default()
            .push(p);
    }

    if players_by_world.is_empty() {
        return;
    }

    // Group alive enemies by world_id
    let mut enemies_by_world: HashMap<String, Vec<Enemy>> = HashMap::new();
    for e in ctx.db.enemy().iter().filter(|e| e.health > 0.0) {
        enemies_by_world
            .entry(e.world_id.clone())
            .or_default()
            .push(e);
    }

    for (world_id, enemies) in &enemies_by_world {
        let Some(players) = players_by_world.get(world_id) else {
            continue;
        };

        for enemy in enemies {
            // Find nearest player (XZ distance)
            let mut nearest_dist = f32::MAX;
            let mut nearest_pos = (0.0_f32, 0.0_f32);
            for p in players {
                let dx = p.x - enemy.x;
                let dz = p.z - enemy.z;
                let dist = (dx * dx + dz * dz).sqrt();
                if dist < nearest_dist {
                    nearest_dist = dist;
                    nearest_pos = (p.x, p.z);
                }
            }

            // Check attack cooldown
            let cooldown_micros = (defaults::ENEMY_ATTACK_COOLDOWN * 1_000_000.0) as i64;
            let attack_cooldown_ready = (now - enemy.last_attack_time) >= cooldown_micros;

            let decision = enemy_ai_decision(nearest_dist, attack_cooldown_ready);

            let mut new_x = enemy.x;
            let mut new_z = enemy.z;
            let mut new_rotation_y = enemy.rotation_y;
            let mut new_last_attack_time = enemy.last_attack_time;

            // Face the player when not idle
            if decision != combat::EnemyBehaviorKind::Idle && nearest_dist > 0.01 {
                let dx = nearest_pos.0 - enemy.x;
                let dz = nearest_pos.1 - enemy.z;
                new_rotation_y = f32::atan2(-dx, -dz);
            }

            // Move toward player when chasing
            if decision == combat::EnemyBehaviorKind::Chase && nearest_dist > 0.01 {
                let dx = nearest_pos.0 - enemy.x;
                let dz = nearest_pos.1 - enemy.z;
                let inv_dist = 1.0 / nearest_dist;
                new_x += dx * inv_dist * defaults::ENEMY_WALK_SPEED * dt;
                new_z += dz * inv_dist * defaults::ENEMY_WALK_SPEED * dt;
            }

            // Enemy-enemy separation — push apart to prevent stacking.
            // Uses squared falloff and proportional (non-normalized) force so
            // enemies near the edge of the radius get negligible push, avoiding
            // visible trembling.
            let mut sep_x = 0.0_f32;
            let mut sep_z = 0.0_f32;
            for other in enemies {
                if other.id == enemy.id {
                    continue;
                }
                let dx = enemy.x - other.x;
                let dz = enemy.z - other.z;
                let dist = (dx * dx + dz * dz).sqrt();
                if dist < defaults::ENEMY_SEPARATION_RADIUS && dist > 0.01 {
                    let inv = 1.0 / dist;
                    let weight = 1.0 - dist / defaults::ENEMY_SEPARATION_RADIUS;
                    sep_x += dx * inv * weight * weight;
                    sep_z += dz * inv * weight * weight;
                }
            }
            let sep_len = (sep_x * sep_x + sep_z * sep_z).sqrt();
            if sep_len > 0.1 {
                new_x += sep_x * defaults::ENEMY_SEPARATION_STRENGTH * dt;
                new_z += sep_z * defaults::ENEMY_SEPARATION_STRENGTH * dt;
            }

            // Reset cooldown on attack
            if decision == combat::EnemyBehaviorKind::Attack {
                new_last_attack_time = now;
            }

            ctx.db.enemy().id().update(Enemy {
                id: enemy.id,
                enemy_type: enemy.enemy_type.clone(),
                world_id: enemy.world_id.clone(),
                x: new_x,
                y: enemy.y,
                z: new_z,
                rotation_y: new_rotation_y,
                animation_state: decision.as_str().to_string(),
                health: enemy.health,
                max_health: enemy.max_health,
                attack_damage: enemy.attack_damage,
                attack_range: enemy.attack_range,
                attack_speed: enemy.attack_speed,
                last_attack_time: new_last_attack_time,
            });
        }
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

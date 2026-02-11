use spacetimedb::Table;
use std::collections::HashMap;
use wasm_fantasia_shared::combat::{self, defaults, enemy_ai_decision};

use crate::schema::*;
use crate::TICK_INTERVAL_MICROS;

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
        if ctx.db.world_pause().world_id().find(world_id).is_some() {
            continue;
        }
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

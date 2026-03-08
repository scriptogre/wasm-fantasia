use game_core::combat::{enemy_defaults, enemy_types, EnemyBehaviorKind};
use spacetimedb::Table;

use crate::schema::*;

/// Spawn ring radii for horde enemies (further than manual spawn).
const SPAWN_RADIUS_MIN: f32 = 30.0;
const SPAWN_RADIUS_MAX: f32 = 50.0;

/// Hard cap on enemy count per world to prevent unbounded accumulation.
const MAX_ENEMIES: usize = 2000;

/// Insert or reset the horde state for a world, activating continuous spawning.
pub fn start_horde(ctx: &spacetimedb::ReducerContext, world_id: u32) {
    let state = HordeState {
        world_id,
        active: true,
        elapsed_secs: 0.0,
        spawn_accumulator: 0.0,
    };
    if ctx.db.horde_state().world_id().find(world_id).is_some() {
        ctx.db.horde_state().world_id().update(state);
    } else {
        ctx.db.horde_state().insert(state);
    }
}

/// Deactivate horde spawning for a world.
pub fn stop_horde(ctx: &spacetimedb::ReducerContext, world_id: u32) {
    if let Some(state) = ctx.db.horde_state().world_id().find(world_id) {
        ctx.db.horde_state().world_id().update(HordeState {
            active: false,
            ..state
        });
    }
}

/// Advance the horde spawner for one tick, spawning enemies as needed.
pub fn tick_horde(ctx: &spacetimedb::ReducerContext, world_id: u32, dt: f32, players: &[Player]) {
    let Some(state) = ctx.db.horde_state().world_id().find(world_id) else {
        return;
    };
    if !state.active || players.is_empty() {
        return;
    }

    let elapsed = state.elapsed_secs + dt;

    // Cap enemy count to prevent unbounded accumulation.
    let enemy_count = ctx.db.enemy().iter().filter(|e| e.world_id == world_id).count();
    if enemy_count >= MAX_ENEMIES {
        ctx.db.horde_state().world_id().update(HordeState {
            world_id,
            active: true,
            elapsed_secs: elapsed,
            spawn_accumulator: 0.0,
        });
        return;
    }

    let spawn_rate = 1.0 + elapsed * 0.05;
    let mut accumulator = state.spawn_accumulator + spawn_rate * dt;

    let seed = ctx.timestamp.to_micros_since_unix_epoch() as u64;
    let mut spawn_index: u64 = 0;

    while accumulator >= 1.0 {
        accumulator -= 1.0;

        // Pick a random player to spawn near
        let player_hash = seed.wrapping_mul(spawn_index.wrapping_add(1)).wrapping_mul(2654435761);
        let player = &players[(player_hash as usize) % players.len()];

        // Pick enemy type based on elapsed time
        let type_roll = ((seed ^ 0xCAFEBABE)
            .wrapping_add(spawn_index)
            .wrapping_mul(6364136223846793005)
            >> 16)
            & 0xFFFF;
        let type_pct = type_roll as f32 / 65535.0;

        let enemy_type = if elapsed < 60.0 {
            enemy_types::BASIC
        } else if elapsed < 120.0 {
            if type_pct < 0.70 {
                enemy_types::BASIC
            } else {
                enemy_types::FAST
            }
        } else if elapsed < 180.0 {
            if type_pct < 0.50 {
                enemy_types::BASIC
            } else if type_pct < 0.80 {
                enemy_types::FAST
            } else {
                enemy_types::BRUTE
            }
        } else if type_pct < 0.40 {
            enemy_types::BASIC
        } else if type_pct < 0.75 {
            enemy_types::FAST
        } else {
            enemy_types::BRUTE
        };

        // Random position in ring around the chosen player
        let h = (seed ^ 0xDEADBEEF)
            .wrapping_add(spawn_index)
            .wrapping_mul(6364136223846793005);
        let angle = (h & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
        let radius =
            SPAWN_RADIUS_MIN + ((h >> 16) & 0xFFFF) as f32 / 65535.0 * (SPAWN_RADIUS_MAX - SPAWN_RADIUS_MIN);

        let stats = enemy_defaults(enemy_type);

        ctx.db.enemy().insert(Enemy {
            id: 0,
            enemy_type,
            world_id,
            x: player.x + angle.cos() * radius,
            y: player.y,
            z: player.z + angle.sin() * radius,
            rotation_y: 0.0,
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
            animation_state: EnemyBehaviorKind::IDLE,
            health: stats.health,
            max_health: stats.health,
            attack_damage: stats.damage,
            attack_range: stats.attack_range,
            attack_speed: stats.attack_speed,
            last_attack_time: 0,
        });

        spawn_index += 1;
    }

    ctx.db.horde_state().world_id().update(HordeState {
        world_id,
        active: true,
        elapsed_secs: elapsed,
        spawn_accumulator: accumulator,
    });
}

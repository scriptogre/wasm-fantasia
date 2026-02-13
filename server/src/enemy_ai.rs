use avian3d::prelude::*;
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
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
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
/// Uses avian3d PhysicsWorld for physics-based movement and knockback.
///
/// The physics world is recreated each tick from DB state. Persistent
/// in-memory state is not viable because SpacetimeDB may dispatch reducers
/// across multiple WASM module instances, each with independent memory.
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

    // Collect knockback impulses by world
    let mut impulses_by_world: HashMap<String, Vec<KnockbackImpulse>> = HashMap::new();
    for impulse in ctx.db.knockback_impulse().iter() {
        impulses_by_world
            .entry(impulse.world_id.clone())
            .or_default()
            .push(impulse);
    }

    for (world_id, enemies) in &enemies_by_world {
        if ctx.db.world_pause().world_id().find(world_id).is_some() {
            continue;
        }
        let Some(players) = players_by_world.get(world_id) else {
            continue;
        };

        // Create a physics world for this tick
        let mut physics = PhysicsWorld::new(PhysicsConfig {
            gravity: Vector::new(0.0, -9.81, 0.0),
            substeps: 4,
            ..Default::default()
        });

        // Add a static floor
        let floor = physics.add_body(RigidBodyBundle::static_body(Vector::ZERO));
        physics.add_collider(floor, ColliderBundle::half_space(Vector::Y));

        // Add enemies as dynamic bodies
        let mut enemy_handles: Vec<(BodyHandle, &Enemy)> = Vec::with_capacity(enemies.len());
        for enemy in enemies {
            let handle = physics.add_body(RigidBodyBundle {
                body_type: RigidBodyType::Dynamic,
                position: Vector::new(enemy.x, enemy.y, enemy.z),
                linear_velocity: Vector::new(enemy.velocity_x, enemy.velocity_y, enemy.velocity_z),
                mass: 50.0,
                ..Default::default()
            });
            physics.add_collider(handle, ColliderBundle::capsule(0.5, 1.0));
            enemy_handles.push((handle, enemy));
        }

        // Apply AI-driven velocities and knockback impulses
        for (handle, enemy) in &enemy_handles {
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

            // Apply knockback impulses for this enemy (before AI velocity so
            // we can skip chase when being knocked back)
            let has_knockback = impulses_by_world
                .get(world_id)
                .is_some_and(|impulses| {
                    impulses.iter().any(|i| i.enemy_id == enemy.id)
                });
            if let Some(impulses) = impulses_by_world.get(world_id) {
                for impulse in impulses.iter().filter(|i| i.enemy_id == enemy.id) {
                    physics.apply_impulse(
                        *handle,
                        Vector::new(impulse.impulse_x, impulse.impulse_y, impulse.impulse_z),
                    );
                }
            }

            // Move toward player when chasing — but skip when being knocked
            // back so the impulse isn't immediately overridden by chase velocity.
            if !has_knockback
                && decision == combat::EnemyBehaviorKind::Chase
                && nearest_dist > 0.01
            {
                let dx = nearest_pos.0 - enemy.x;
                let dz = nearest_pos.1 - enemy.z;
                let inv_dist = 1.0 / nearest_dist;
                let move_x = dx * inv_dist * defaults::ENEMY_WALK_SPEED;
                let move_z = dz * inv_dist * defaults::ENEMY_WALK_SPEED;
                physics.set_linear_velocity(
                    *handle,
                    Vector::new(move_x, physics.body(*handle).linear_velocity().y, move_z),
                );
            }
        }

        // Step physics
        let _result = physics.step(dt);

        // Write back physics state to DB and update AI state
        for (handle, enemy) in &enemy_handles {
            let body = physics.body(*handle);

            // AI decision (recomputed — cheap)
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
            let cooldown_micros = (defaults::ENEMY_ATTACK_COOLDOWN * 1_000_000.0) as i64;
            let attack_cooldown_ready = (now - enemy.last_attack_time) >= cooldown_micros;
            let decision = enemy_ai_decision(nearest_dist, attack_cooldown_ready);

            let mut new_rotation_y = enemy.rotation_y;
            if decision != combat::EnemyBehaviorKind::Idle && nearest_dist > 0.01 {
                let dx = nearest_pos.0 - enemy.x;
                let dz = nearest_pos.1 - enemy.z;
                new_rotation_y = f32::atan2(-dx, -dz);
            }

            let new_last_attack_time = if decision == combat::EnemyBehaviorKind::Attack {
                now
            } else {
                enemy.last_attack_time
            };

            let pos = body.position();
            let vel = body.linear_velocity();

            ctx.db.enemy().id().update(Enemy {
                id: enemy.id,
                enemy_type: enemy.enemy_type.clone(),
                world_id: enemy.world_id.clone(),
                x: pos.x,
                y: pos.y,
                z: pos.z,
                rotation_y: new_rotation_y,
                velocity_x: vel.x,
                velocity_y: vel.y,
                velocity_z: vel.z,
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

    // Delete consumed knockback impulses
    for impulse in ctx.db.knockback_impulse().iter().collect::<Vec<_>>() {
        ctx.db.knockback_impulse().id().delete(impulse.id);
    }
}

use avian3d::prelude::*;
use game_core::combat::{self, defaults, enemy_ai_decision, enemy_types, EnemyBehaviorKind};
use spacetimedb::Table;
use std::collections::HashMap;

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
    let count = 500;

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
            enemy_type: enemy_types::BASIC,
            world_id: world_id.clone(),
            x: x + angle.cos() * radius,
            y,
            z: z + angle.sin() * radius,
            rotation_y: 0.0,
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
            animation_state: EnemyBehaviorKind::IDLE,
            health: defaults::ENEMY_HEALTH,
            max_health: defaults::ENEMY_HEALTH,
            attack_damage: defaults::ENEMY_ATTACK_DAMAGE,
            attack_range: defaults::ENEMY_ATTACK_RANGE,
            attack_speed: 1.0,
            last_attack_time: 0,
        });
    }
}

/// Delete all enemies in the caller's world.
#[spacetimedb::reducer]
pub fn clear_enemies(ctx: &spacetimedb::ReducerContext) {
    let Some(player) = ctx.db.player().identity().find(ctx.sender) else {
        return;
    };

    let world_id = player.world_id;
    let enemy_ids: Vec<u64> = ctx
        .db
        .enemy()
        .iter()
        .filter(|e| e.world_id == world_id)
        .map(|e| e.id)
        .collect();

    let count = enemy_ids.len();
    for id in enemy_ids {
        ctx.db.enemy().id().delete(id);
    }

    spacetimedb::log::info!("Cleared {} enemies from world {}", count, world_id);
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
        // Only clone world_id when inserting a new key (avoids clone per row)
        if let Some(vec) = players_by_world.get_mut(&p.world_id) {
            vec.push(p);
        } else {
            players_by_world.insert(p.world_id.clone(), vec![p]);
        }
    }

    if players_by_world.is_empty() {
        return;
    }

    // Group alive enemies by world_id
    let mut enemies_by_world: HashMap<String, Vec<Enemy>> = HashMap::new();
    for e in ctx.db.enemy().iter().filter(|e| e.health > 0.0) {
        if let Some(vec) = enemies_by_world.get_mut(&e.world_id) {
            vec.push(e);
        } else {
            enemies_by_world.insert(e.world_id.clone(), vec![e]);
        }
    }

    // S4: Index knockback impulses by enemy_id for O(1) lookup
    let mut impulses_by_enemy: HashMap<u64, Vec<KnockbackImpulse>> = HashMap::new();
    for impulse in ctx.db.knockback_impulse().iter() {
        impulses_by_enemy
            .entry(impulse.enemy_id)
            .or_default()
            .push(impulse);
    }

    let cooldown_micros = (defaults::ENEMY_ATTACK_COOLDOWN * 1_000_000.0) as i64;

    for (world_id_key, enemies) in &enemies_by_world {
        if ctx.db.world_pause().world_id().find(world_id_key).is_some() {
            continue;
        }
        let Some(players) = players_by_world.get(world_id_key) else {
            continue;
        };

        // Clone world_id once per world, not per enemy
        let world_id = world_id_key.clone();

        // S1: Spatial grid for enemy separation (O(N) instead of O(N²))
        let sep_radius = defaults::ENEMY_SEPARATION_RADIUS;
        let sep_radius_sq = sep_radius * sep_radius;
        let sep_strength = defaults::ENEMY_SEPARATION_STRENGTH;
        let inv_cell = 1.0 / sep_radius;

        let mut grid: HashMap<(i32, i32), Vec<usize>> =
            HashMap::with_capacity(enemies.len());
        for (idx, enemy) in enemies.iter().enumerate() {
            let cx = (enemy.x * inv_cell).floor() as i32;
            let cz = (enemy.z * inv_cell).floor() as i32;
            grid.entry((cx, cz)).or_default().push(idx);
        }

        let mut separation: Vec<(f32, f32)> = vec![(0.0, 0.0); enemies.len()];
        for (&(cx, cz), cell_indices) in &grid {
            for &i in cell_indices {
                for dcx in -1..=1 {
                    for dcz in -1..=1 {
                        if let Some(neighbor_indices) = grid.get(&(cx + dcx, cz + dcz)) {
                            for &j in neighbor_indices {
                                if j <= i {
                                    continue;
                                }
                                let dx = enemies[i].x - enemies[j].x;
                                let dz = enemies[i].z - enemies[j].z;
                                let dist_sq = dx * dx + dz * dz;
                                if dist_sq < sep_radius_sq && dist_sq > 1e-6 {
                                    let dist = dist_sq.sqrt();
                                    let overlap = 1.0 - dist / sep_radius;
                                    let push = overlap * sep_strength / dist;
                                    let px = dx * push;
                                    let pz = dz * push;
                                    separation[i].0 += px;
                                    separation[i].1 += pz;
                                    separation[j].0 -= px;
                                    separation[j].1 -= pz;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Pre-compute nearest player + AI decision for each enemy.
        // Also classify enemies as grounded (simple XZ movement) or airborne
        // (need full physics). This avoids adding ~5000 bodies to PhysicsWorld
        // when only ~0-50 are actually airborne from knockback.
        struct EnemyUpdate {
            decision: combat::EnemyBehaviorKind,
            nearest_dist: f32,
            nearest_pos: (f32, f32),
            new_x: f32,
            new_y: f32,
            new_z: f32,
            new_vx: f32,
            new_vy: f32,
            new_vz: f32,
        }

        let mut updates: Vec<EnemyUpdate> = Vec::with_capacity(enemies.len());

        // Collect airborne enemy indices + their impulses for physics
        let mut airborne_indices: Vec<usize> = Vec::new();

        for (idx, enemy) in enemies.iter().enumerate() {
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

            let attack_cooldown_ready = (now - enemy.last_attack_time) >= cooldown_micros;
            let decision = enemy_ai_decision(nearest_dist, attack_cooldown_ready);

            let has_knockback = impulses_by_enemy.contains_key(&enemy.id);
            let is_airborne = has_knockback || enemy.velocity_y.abs() > 0.1;

            // Chase velocity (only when grounded and not being knocked back)
            let (mut vx, mut vz) = (0.0_f32, 0.0_f32);
            if !has_knockback && decision == combat::EnemyBehaviorKind::Chase && nearest_dist > 0.01
            {
                let dx = nearest_pos.0 - enemy.x;
                let dz = nearest_pos.1 - enemy.z;
                let inv_dist = 1.0 / nearest_dist;
                vx = dx * inv_dist * defaults::ENEMY_WALK_SPEED;
                vz = dz * inv_dist * defaults::ENEMY_WALK_SPEED;
            }

            // Add separation push
            vx += separation[idx].0;
            vz += separation[idx].1;

            if is_airborne {
                // This enemy needs physics — will be processed below
                airborne_indices.push(idx);
                updates.push(EnemyUpdate {
                    decision,
                    nearest_dist,
                    nearest_pos,
                    // Placeholder — physics will overwrite these
                    new_x: enemy.x,
                    new_y: enemy.y,
                    new_z: enemy.z,
                    new_vx: vx,
                    new_vy: enemy.velocity_y,
                    new_vz: vz,
                });
            } else {
                // Grounded: simple XZ movement, y stays the same
                updates.push(EnemyUpdate {
                    decision,
                    nearest_dist,
                    nearest_pos,
                    new_x: enemy.x + vx * dt,
                    new_y: enemy.y,
                    new_z: enemy.z + vz * dt,
                    new_vx: vx,
                    new_vy: 0.0,
                    new_vz: vz,
                });
            }
        }

        // Only create PhysicsWorld if there are airborne enemies
        if !airborne_indices.is_empty() {
            let mut physics = PhysicsWorld::new(PhysicsConfig {
                gravity: Vector::new(0.0, -30.0, 0.0),
                substeps: 4,
                ..Default::default()
            });

            let floor = physics.add_body(RigidBodyBundle::static_body(Vector::ZERO));
            physics.add_collider(floor, ColliderBundle::half_space(Vector::Y));

            let mut handles: Vec<BodyHandle> = Vec::with_capacity(airborne_indices.len());
            for &idx in &airborne_indices {
                let enemy = &enemies[idx];
                let update = &updates[idx];
                let handle = physics.add_body(RigidBodyBundle {
                    body_type: RigidBodyType::Dynamic,
                    position: Vector::new(enemy.x, enemy.y, enemy.z),
                    linear_velocity: Vector::new(
                        update.new_vx,
                        enemy.velocity_y,
                        update.new_vz,
                    ),
                    mass: defaults::ENEMY_MASS,
                    ..Default::default()
                });
                physics.add_collider(handle, ColliderBundle::capsule(0.5, 1.0));

                // Apply knockback impulses
                if let Some(impulses) = impulses_by_enemy.get(&enemy.id) {
                    for impulse in impulses {
                        physics.apply_impulse(
                            handle,
                            Vector::new(impulse.impulse_x, impulse.impulse_y, impulse.impulse_z),
                        );
                    }
                }

                handles.push(handle);
            }

            let _result = physics.step(dt);

            // Write physics results back into updates
            for (i, &idx) in airborne_indices.iter().enumerate() {
                let body = physics.body(handles[i]);
                let pos = body.position();
                let vel = body.linear_velocity();
                updates[idx].new_x = pos.x;
                updates[idx].new_y = pos.y;
                updates[idx].new_z = pos.z;
                updates[idx].new_vx = vel.x;
                updates[idx].new_vy = vel.y;
                updates[idx].new_vz = vel.z;
            }
        }

        // Write back to DB
        for (idx, enemy) in enemies.iter().enumerate() {
            let update = &updates[idx];

            let mut new_rotation_y = enemy.rotation_y;
            if update.decision != combat::EnemyBehaviorKind::Idle && update.nearest_dist > 0.01 {
                let dx = update.nearest_pos.0 - enemy.x;
                let dz = update.nearest_pos.1 - enemy.z;
                new_rotation_y = f32::atan2(-dx, -dz);
            }

            let new_last_attack_time = if update.decision == combat::EnemyBehaviorKind::Attack {
                now
            } else {
                enemy.last_attack_time
            };

            let new_anim = update.decision.as_u8();

            // S2: Skip DB write if nothing meaningful changed
            let pos_changed = (update.new_x - enemy.x).abs() > 0.01
                || (update.new_y - enemy.y).abs() > 0.01
                || (update.new_z - enemy.z).abs() > 0.01;
            let anim_changed = enemy.animation_state != new_anim;
            let attack_changed = new_last_attack_time != enemy.last_attack_time;

            if !pos_changed && !anim_changed && !attack_changed {
                continue;
            }

            ctx.db.enemy().id().update(Enemy {
                id: enemy.id,
                enemy_type: enemy.enemy_type,
                world_id: world_id.clone(),
                x: update.new_x,
                y: update.new_y,
                z: update.new_z,
                rotation_y: new_rotation_y,
                velocity_x: update.new_vx,
                velocity_y: update.new_vy,
                velocity_z: update.new_vz,
                animation_state: new_anim,
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
    let impulse_ids: Vec<u64> = ctx.db.knockback_impulse().iter().map(|i| i.id).collect();
    for id in impulse_ids {
        ctx.db.knockback_impulse().id().delete(id);
    }
}

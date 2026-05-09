use avian3d::prelude::*;
use game_core::combat::{
    self, EnemyBehaviorKind, defaults, enemy_ai_decision, enemy_animation_state, enemy_types,
};
use spacetimedb::Table;
use std::collections::HashMap;

/// Half-neighbor offsets for symmetric pair visitation.
/// Each pair (i, j) is visited exactly once — no `j <= i` skip needed.
const HALF_NEIGHBORS: [(i32, i32); 5] = [(0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

use crate::TICK_INTERVAL_MICROS;
use crate::schema::*;

/// Spawn a pack of enemies at the given position and facing direction.
#[spacetimedb::reducer]
pub fn spawn_enemies(
    ctx: &spacetimedb::ReducerContext,
    x: f32,
    _y: f32,
    z: f32,
    _forward_x: f32,
    _forward_z: f32,
) {
    let Some(player) = ctx.db.player().identity().find(ctx.sender()) else {
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
            world_id,
            x: x + angle.cos() * radius,
            y: defaults::ENEMY_SPAWN_Y,
            z: z + angle.sin() * radius,
            rotation_y: 0.0,
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
            animation_state: EnemyBehaviorKind::IDLE,
            state_start_time: 0,
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
    let Some(player) = ctx.db.player().identity().find(ctx.sender()) else {
        return;
    };

    let world_id = player.world_id;
    let enemy_ids: Vec<u64> = ctx
        .db
        .enemy()
        .world_id()
        .filter(&world_id)
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
    let mut players_by_world: HashMap<u32, Vec<Player>> = HashMap::new();
    for p in ctx
        .db
        .player()
        .iter()
        .filter(|p| p.online && p.health > 0.0)
    {
        players_by_world.entry(p.world_id).or_default().push(p);
    }

    if players_by_world.is_empty() {
        return;
    }

    // Group alive enemies by world_id
    let mut enemies_by_world: HashMap<u32, Vec<Enemy>> = HashMap::new();
    for e in ctx.db.enemy().iter().filter(|e| e.health > 0.0) {
        enemies_by_world.entry(e.world_id).or_default().push(e);
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

    for (&world_id, enemies) in &enemies_by_world {
        if ctx.db.world_pause().world_id().find(world_id).is_some() {
            continue;
        }
        let Some(players) = players_by_world.get(&world_id) else {
            continue;
        };

        // S1: Spatial grid for enemy separation (O(N) instead of O(N²))
        let sep_radius = defaults::ENEMY_SEPARATION_RADIUS;
        let sep_radius_sq = sep_radius * sep_radius;
        let sep_strength = defaults::ENEMY_SEPARATION_STRENGTH;
        let inv_cell = 1.0 / sep_radius;

        // Pass 1: compute cell coords + bounding box
        let mut cell_coords: Vec<(i32, i32)> = Vec::with_capacity(enemies.len());
        let (mut min_cx, mut max_cx) = (i32::MAX, i32::MIN);
        let (mut min_cz, mut max_cz) = (i32::MAX, i32::MIN);
        for enemy in enemies.iter() {
            let cx = (enemy.x * inv_cell).floor() as i32;
            let cz = (enemy.z * inv_cell).floor() as i32;
            cell_coords.push((cx, cz));
            min_cx = min_cx.min(cx);
            max_cx = max_cx.max(cx);
            min_cz = min_cz.min(cz);
            max_cz = max_cz.max(cz);
        }

        let grid_w = (max_cx - min_cx + 1) as usize;
        let grid_h = (max_cz - min_cz + 1) as usize;

        let mut separation: Vec<(f32, f32)> = vec![(0.0, 0.0); enemies.len()];

        if grid_w * grid_h <= 100_000 {
            // Flat counting-sort grid
            let grid_size = grid_w * grid_h;

            // Pass 2: count enemies per cell
            let mut counts = vec![0u32; grid_size];
            for &(cx, cz) in &cell_coords {
                let flat = (cz - min_cz) as usize * grid_w + (cx - min_cx) as usize;
                counts[flat] += 1;
            }

            // Pass 3: prefix sum → start offsets
            let mut offsets = vec![0u32; grid_size + 1];
            for i in 0..grid_size {
                offsets[i + 1] = offsets[i] + counts[i];
            }

            // Pass 4: place enemy indices into sorted array
            let mut sorted = vec![0usize; enemies.len()];
            let mut write_pos = offsets.clone();
            for (idx, &(cx, cz)) in cell_coords.iter().enumerate() {
                let flat = (cz - min_cz) as usize * grid_w + (cx - min_cx) as usize;
                sorted[write_pos[flat] as usize] = idx;
                write_pos[flat] += 1;
            }

            // Separation with half-neighbor pattern (5 lookups per cell, each pair once)
            for gz in 0..grid_h as i32 {
                for gx in 0..grid_w as i32 {
                    let cell_flat = gz as usize * grid_w + gx as usize;
                    let cell_start = offsets[cell_flat] as usize;
                    let cell_end = offsets[cell_flat + 1] as usize;
                    if cell_start == cell_end {
                        continue;
                    }

                    for &(dx, dz) in &HALF_NEIGHBORS {
                        let nx = gx + dx;
                        let nz = gz + dz;
                        if nx < 0 || nx >= grid_w as i32 || nz < 0 || nz >= grid_h as i32 {
                            continue;
                        }
                        let nb_flat = nz as usize * grid_w + nx as usize;
                        let nb_start = offsets[nb_flat] as usize;
                        let nb_end = offsets[nb_flat + 1] as usize;
                        if nb_start == nb_end {
                            continue;
                        }

                        if cell_flat == nb_flat {
                            // Same cell: check all unique pairs within
                            for ai in cell_start..cell_end {
                                for bi in (ai + 1)..cell_end {
                                    let i = sorted[ai];
                                    let j = sorted[bi];
                                    let edx = enemies[i].x - enemies[j].x;
                                    let edz = enemies[i].z - enemies[j].z;
                                    let dist_sq = edx * edx + edz * edz;
                                    if dist_sq < sep_radius_sq && dist_sq > 1e-6 {
                                        let dist = dist_sq.sqrt();
                                        let overlap = 1.0 - dist / sep_radius;
                                        let push = overlap * sep_strength / dist;
                                        let px = edx * push;
                                        let pz = edz * push;
                                        separation[i].0 += px;
                                        separation[i].1 += pz;
                                        separation[j].0 -= px;
                                        separation[j].1 -= pz;
                                    }
                                }
                            }
                        } else {
                            // Different cells: all pairs between cell and neighbor
                            for ai in cell_start..cell_end {
                                for bi in nb_start..nb_end {
                                    let i = sorted[ai];
                                    let j = sorted[bi];
                                    let edx = enemies[i].x - enemies[j].x;
                                    let edz = enemies[i].z - enemies[j].z;
                                    let dist_sq = edx * edx + edz * edz;
                                    if dist_sq < sep_radius_sq && dist_sq > 1e-6 {
                                        let dist = dist_sq.sqrt();
                                        let overlap = 1.0 - dist / sep_radius;
                                        let push = overlap * sep_strength / dist;
                                        let px = edx * push;
                                        let pz = edz * push;
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
        } else {
            // Fallback: HashMap grid for extremely sparse distributions
            let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::with_capacity(enemies.len());
            for (idx, &(cx, cz)) in cell_coords.iter().enumerate() {
                grid.entry((cx, cz)).or_default().push(idx);
            }
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
        }

        // Pre-compute nearest player + AI decision for each enemy.
        // Also classify enemies as grounded (simple XZ movement) or airborne
        // (need full physics). This avoids adding ~5000 bodies to PhysicsWorld
        // when only ~0-50 are actually airborne from knockback.
        struct EnemyUpdate {
            decision: combat::EnemyBehaviorKind,
            current_state: combat::EnemyBehaviorKind,
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
        let mut airborne_indices: Vec<usize> = Vec::with_capacity(64);

        // Pending player damage from enemy attacks hitting their target
        let mut pending_player_damage: Vec<(spacetimedb::Identity, f32)> = Vec::new();

        for (idx, enemy) in enemies.iter().enumerate() {
            let mut nearest_dist_sq = f32::MAX;
            let mut nearest_pos = (0.0_f32, 0.0_f32);
            let mut nearest_player_identity = players[0].identity;
            for p in players {
                let dx = p.x - enemy.x;
                let dz = p.z - enemy.z;
                let dist_sq = dx * dx + dz * dz;
                if dist_sq < nearest_dist_sq {
                    nearest_dist_sq = dist_sq;
                    nearest_pos = (p.x, p.z);
                    nearest_player_identity = p.identity;
                }
            }
            let nearest_dist = nearest_dist_sq.sqrt();

            let current_state = EnemyBehaviorKind::from_u8(enemy.animation_state);
            let state_elapsed = (now - enemy.state_start_time) as f32 / 1_000_000.0;
            let attack_cooldown_ready = (now - enemy.last_attack_time) >= cooldown_micros;
            let decision = enemy_ai_decision(
                current_state,
                state_elapsed,
                nearest_dist,
                attack_cooldown_ready,
            );

            // Check if enemy is at the hit frame of its attack
            if current_state == EnemyBehaviorKind::Attack {
                let prev_elapsed = state_elapsed - dt;
                if prev_elapsed < defaults::ENEMY_ATTACK_HIT
                    && state_elapsed >= defaults::ENEMY_ATTACK_HIT
                {
                    if nearest_dist <= defaults::ENEMY_ATTACK_RANGE {
                        pending_player_damage.push((nearest_player_identity, enemy.attack_damage));
                    }
                }
            }

            let has_knockback = impulses_by_enemy.contains_key(&enemy.id);
            let is_airborne = has_knockback || enemy.velocity_y.abs() > 0.1;

            // Chase velocity (only when grounded, not knocked back, and not attacking)
            let (mut vx, mut vz) = (0.0_f32, 0.0_f32);
            if !has_knockback && decision == combat::EnemyBehaviorKind::Chase && nearest_dist > 0.01
            {
                let dx = nearest_pos.0 - enemy.x;
                let dz = nearest_pos.1 - enemy.z;
                let inv_dist = 1.0 / nearest_dist;
                vx = dx * inv_dist * defaults::ENEMY_WALK_SPEED;
                vz = dz * inv_dist * defaults::ENEMY_WALK_SPEED;
            }

            // Add separation force, but when not chasing, remove the component
            // that would push the enemy toward the player. Without this, a horde
            // converging on the player stacks separation forces and pushes front
            // enemies straight through the player.
            let sep_scale = if decision == EnemyBehaviorKind::Attack {
                0.3
            } else {
                1.0
            };
            let mut sep_x = separation[idx].0 * sep_scale;
            let mut sep_z = separation[idx].1 * sep_scale;

            if decision != combat::EnemyBehaviorKind::Chase && nearest_dist > 0.01 {
                let to_player_x = (nearest_pos.0 - enemy.x) / nearest_dist;
                let to_player_z = (nearest_pos.1 - enemy.z) / nearest_dist;
                let dot = sep_x * to_player_x + sep_z * to_player_z;
                if dot > 0.0 {
                    // Remove player-ward component — only allow lateral/outward separation
                    sep_x -= dot * to_player_x;
                    sep_z -= dot * to_player_z;
                }
            }

            vx += sep_x;
            vz += sep_z;

            if is_airborne {
                // This enemy needs physics — will be processed below
                airborne_indices.push(idx);
                updates.push(EnemyUpdate {
                    decision,
                    current_state,
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
                    current_state,
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
                    linear_velocity: Vector::new(update.new_vx, enemy.velocity_y, update.new_vz),
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

        // Apply accumulated enemy damage to players
        for (identity, damage) in &pending_player_damage {
            if let Some(player) = ctx.db.player().identity().find(*identity) {
                let new_health = (player.health - damage).max(0.0);
                ctx.db.player().identity().update(Player {
                    health: new_health,
                    ..player
                });
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

            // Only set last_attack_time when the attack COMPLETES (transitions away from Attack)
            let new_last_attack_time = if update.current_state == combat::EnemyBehaviorKind::Attack
                && update.decision != combat::EnemyBehaviorKind::Attack
            {
                now // Attack just finished — start cooldown
            } else {
                enemy.last_attack_time
            };

            let planar_speed = update.new_vx.hypot(update.new_vz);
            let new_anim = enemy_animation_state(update.decision, planar_speed).as_u8();

            // Update state_start_time only when the state actually changes
            let state_start_time = if new_anim != enemy.animation_state {
                now
            } else {
                enemy.state_start_time
            };

            // S2: Skip DB write if nothing meaningful changed
            let pos_changed = (update.new_x - enemy.x).abs() > 0.01
                || (update.new_y - enemy.y).abs() > 0.01
                || (update.new_z - enemy.z).abs() > 0.01;
            let anim_changed = enemy.animation_state != new_anim;
            let attack_changed = new_last_attack_time != enemy.last_attack_time;
            let state_time_changed = state_start_time != enemy.state_start_time;

            if !pos_changed && !anim_changed && !attack_changed && !state_time_changed {
                continue;
            }

            ctx.db.enemy().id().update(Enemy {
                id: enemy.id,
                enemy_type: enemy.enemy_type,
                world_id,
                x: update.new_x,
                y: update.new_y,
                z: update.new_z,
                rotation_y: new_rotation_y,
                velocity_x: update.new_vx,
                velocity_y: update.new_vy,
                velocity_z: update.new_vz,
                animation_state: new_anim,
                state_start_time,
                health: enemy.health,
                max_health: enemy.max_health,
                attack_damage: enemy.attack_damage,
                attack_range: enemy.attack_range,
                attack_speed: enemy.attack_speed,
                last_attack_time: new_last_attack_time,
            });
        }
    }

    // Tick horde spawner for each active world with players.
    // Pass the already-collected enemy count to avoid a second full table scan.
    for (&world_id, players) in &players_by_world {
        if ctx.db.world_pause().world_id().find(world_id).is_some() {
            continue;
        }
        let enemy_count = enemies_by_world.get(&world_id).map_or(0, |v| v.len());
        crate::horde::tick_horde(ctx, world_id, dt, players, enemy_count);
    }

    // Delete consumed knockback impulses
    let impulse_ids: Vec<u64> = ctx.db.knockback_impulse().iter().map(|i| i.id).collect();
    for id in impulse_ids {
        ctx.db.knockback_impulse().id().delete(id);
    }
}

//! Benchmarks simulating the server game_tick data preparation.
//!
//! This isolates the parts of game_tick that our optimizations affect:
//! 1. Grouping entities by world_id (String cloning)
//! 2. Spatial grid construction
//! 3. AI decision-making per enemy
//! 4. Building the update struct (String clone vs u8 copy)
//!
//! Physics stepping (avian3d) is excluded — it's unaffected by our changes.
//!
//! Run: cargo bench -p game-core --bench game_tick

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use game_core::combat::{EnemyBehaviorKind, defaults, enemy_ai_decision};
use std::collections::HashMap;

/// Half-neighbor offsets for symmetric pair visitation.
const HALF_NEIGHBORS: [(i32, i32); 5] = [(0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

// ============================================================================
// Mirror structs — same layout as server schema, without SpacetimeDB deps
// ============================================================================

/// Enemy with String fields (old schema)
#[derive(Clone)]
struct EnemyOld {
    id: u64,
    enemy_type: String,
    world_id: String,
    x: f32,
    y: f32,
    z: f32,
    rotation_y: f32,
    velocity_x: f32,
    velocity_y: f32,
    velocity_z: f32,
    animation_state: String,
    health: f32,
    max_health: f32,
    attack_damage: f32,
    attack_range: f32,
    attack_speed: f32,
    last_attack_time: i64,
}

/// Enemy with u8 fields (new schema)
#[derive(Clone)]
struct EnemyNew {
    id: u64,
    enemy_type: u8,
    world_id: u32,
    x: f32,
    y: f32,
    z: f32,
    rotation_y: f32,
    velocity_x: f32,
    velocity_y: f32,
    velocity_z: f32,
    animation_state: u8,
    health: f32,
    max_health: f32,
    attack_damage: f32,
    attack_range: f32,
    attack_speed: f32,
    last_attack_time: i64,
}

#[derive(Clone, Copy)]
struct Player {
    x: f32,
    z: f32,
    world_id: u32,
}

// ============================================================================
// Data generation
// ============================================================================

fn make_players(count: usize, world_id: u32) -> Vec<Player> {
    (0..count)
        .map(|_| Player {
            x: 0.0,
            z: 0.0,
            world_id,
        })
        .collect()
}

fn make_enemies_old(count: usize, world_id: &str) -> Vec<EnemyOld> {
    let seed = 12345u64;
    (0..count)
        .map(|i| {
            let h = seed
                .wrapping_add(i as u64)
                .wrapping_mul(6364136223846793005);
            let angle = (h & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
            let radius = 5.0 + ((h >> 16) & 0xFFFF) as f32 / 65535.0 * 15.0;
            EnemyOld {
                id: i as u64,
                enemy_type: "basic".to_string(),
                world_id: world_id.to_string(),
                x: angle.cos() * radius,
                y: 0.0,
                z: angle.sin() * radius,
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
            }
        })
        .collect()
}

fn make_enemies_new(count: usize, world_id: u32) -> Vec<EnemyNew> {
    let seed = 12345u64;
    (0..count)
        .map(|i| {
            let h = seed
                .wrapping_add(i as u64)
                .wrapping_mul(6364136223846793005);
            let angle = (h & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
            let radius = 5.0 + ((h >> 16) & 0xFFFF) as f32 / 65535.0 * 15.0;
            EnemyNew {
                id: i as u64,
                enemy_type: 0,
                world_id,
                x: angle.cos() * radius,
                y: 0.0,
                z: angle.sin() * radius,
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
            }
        })
        .collect()
}

// ============================================================================
// Benchmark: grouping by world_id (old: entry().clone() vs new: get_mut/insert)
// ============================================================================

fn bench_grouping(c: &mut Criterion) {
    let mut group = c.benchmark_group("grouping_by_world");

    for count in [1000, 5000, 10000] {
        let enemies_old = make_enemies_old(count, "world1");
        let enemies_new = make_enemies_new(count, 1);

        // Old approach: entry().or_default() clones world_id every row
        group.bench_with_input(
            BenchmarkId::new("old_entry_clone", count),
            &enemies_old,
            |b, enemies| {
                b.iter(|| {
                    let mut by_world: HashMap<String, Vec<&EnemyOld>> = HashMap::new();
                    for e in enemies {
                        by_world.entry(e.world_id.clone()).or_default().push(e);
                    }
                    black_box(&by_world);
                })
            },
        );

        // New approach: u32 world_id — no allocation, just copy
        group.bench_with_input(
            BenchmarkId::new("new_u32", count),
            &enemies_new,
            |b, enemies| {
                b.iter(|| {
                    let mut by_world: HashMap<u32, Vec<&EnemyNew>> = HashMap::new();
                    for e in enemies {
                        by_world.entry(e.world_id).or_default().push(e);
                    }
                    black_box(&by_world);
                })
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark: spatial grid + separation (unaffected, for baseline)
// ============================================================================

fn bench_spatial_grid(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial_grid");

    for count in [1000, 5000, 10000] {
        let enemies = make_enemies_new(count, 1);

        // Old: HashMap grid with 9-neighbor lookup
        group.bench_with_input(
            BenchmarkId::new("old_hashmap", count),
            &enemies,
            |b, enemies| {
                let sep_radius = defaults::ENEMY_SEPARATION_RADIUS;
                let sep_radius_sq = sep_radius * sep_radius;
                let sep_strength = defaults::ENEMY_SEPARATION_STRENGTH;
                let inv_cell = 1.0 / sep_radius;

                b.iter(|| {
                    let mut grid: HashMap<(i32, i32), Vec<usize>> =
                        HashMap::with_capacity(enemies.len());
                    for (idx, enemy) in enemies.iter().enumerate() {
                        let cx = (enemy.x * inv_cell).floor() as i32;
                        let cz = (enemy.z * inv_cell).floor() as i32;
                        grid.entry((cx, cz)).or_default().push(idx);
                    }

                    let mut separation = vec![(0.0f32, 0.0f32); enemies.len()];
                    for (&(cx, cz), cell_indices) in &grid {
                        for &i in cell_indices {
                            for dcx in -1..=1 {
                                for dcz in -1..=1 {
                                    if let Some(neighbors) = grid.get(&(cx + dcx, cz + dcz)) {
                                        for &j in neighbors {
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
                    black_box(&separation);
                })
            },
        );

        // New: flat counting-sort grid with half-neighbor pattern
        group.bench_with_input(
            BenchmarkId::new("new_flat_grid", count),
            &enemies,
            |b, enemies| {
                let sep_radius = defaults::ENEMY_SEPARATION_RADIUS;
                let sep_radius_sq = sep_radius * sep_radius;
                let sep_strength = defaults::ENEMY_SEPARATION_STRENGTH;
                let inv_cell = 1.0 / sep_radius;

                b.iter(|| {
                    flat_grid_separation(enemies, inv_cell, sep_radius_sq, sep_strength);
                })
            },
        );
    }

    group.finish();
}

/// Flat counting-sort grid with half-neighbor separation.
/// Extracted so both `bench_spatial_grid` and `bench_full_tick_simulation` can share it.
fn flat_grid_separation(
    enemies: &[EnemyNew],
    inv_cell: f32,
    sep_radius_sq: f32,
    sep_strength: f32,
) -> Vec<(f32, f32)> {
    // Pass 1: cell coords + bounding box
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
    let grid_size = grid_w * grid_h;

    let mut separation = vec![(0.0f32, 0.0f32); enemies.len()];

    // Pass 2: count
    let mut counts = vec![0u32; grid_size];
    for &(cx, cz) in &cell_coords {
        counts[(cz - min_cz) as usize * grid_w + (cx - min_cx) as usize] += 1;
    }

    // Pass 3: prefix sum
    let mut offsets = vec![0u32; grid_size + 1];
    for i in 0..grid_size {
        offsets[i + 1] = offsets[i] + counts[i];
    }

    // Pass 4: place indices
    let mut sorted = vec![0usize; enemies.len()];
    let mut write_pos = offsets.clone();
    for (idx, &(cx, cz)) in cell_coords.iter().enumerate() {
        let flat = (cz - min_cz) as usize * grid_w + (cx - min_cx) as usize;
        sorted[write_pos[flat] as usize] = idx;
        write_pos[flat] += 1;
    }

    let sep_radius = sep_radius_sq.sqrt();

    // Half-neighbor separation
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

    black_box(separation)
}

// ============================================================================
// Benchmark: AI decisions + velocity computation
// ============================================================================

fn bench_ai_decisions(c: &mut Criterion) {
    let mut group = c.benchmark_group("ai_decisions");

    for count in [1000, 5000, 10000] {
        let enemies = make_enemies_new(count, 1);
        let players = make_players(1, 1);
        let now = 1_000_000_000i64;
        let cooldown_micros = (defaults::ENEMY_ATTACK_COOLDOWN * 1_000_000.0) as i64;

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &enemies,
            |b, enemies| {
                b.iter(|| {
                    let mut decisions = Vec::with_capacity(enemies.len());
                    for enemy in enemies {
                        let mut nearest_dist = f32::MAX;
                        let mut nearest_pos = (0.0f32, 0.0f32);
                        for p in &players {
                            let dx = p.x - enemy.x;
                            let dz = p.z - enemy.z;
                            let dist = (dx * dx + dz * dz).sqrt();
                            if dist < nearest_dist {
                                nearest_dist = dist;
                                nearest_pos = (p.x, p.z);
                            }
                        }
                        let ready = (now - enemy.last_attack_time) >= cooldown_micros;
                        let decision = enemy_ai_decision(EnemyBehaviorKind::Idle, 10.0, nearest_dist, ready);
                        decisions.push((decision, nearest_dist, nearest_pos));
                    }
                    black_box(&decisions);
                })
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark: build update struct (the DB write payload)
// This is where String→u8 matters most — called for every moved enemy.
// ============================================================================

fn bench_build_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_update_struct");

    for count in [1000, 5000, 10000] {
        let enemies_old = make_enemies_old(count, "world1");
        let enemies_new = make_enemies_new(count, 1);

        // Old: clone enemy_type String + clone world_id + allocate animation_state
        group.bench_with_input(
            BenchmarkId::new("old_string", count),
            &enemies_old,
            |b, enemies| {
                b.iter(|| {
                    let mut updates = Vec::with_capacity(enemies.len());
                    for enemy in enemies {
                        updates.push(EnemyOld {
                            id: enemy.id,
                            enemy_type: enemy.enemy_type.clone(),
                            world_id: enemy.world_id.clone(),
                            x: enemy.x + 0.1,
                            y: enemy.y,
                            z: enemy.z + 0.1,
                            rotation_y: 0.5,
                            velocity_x: 1.0,
                            velocity_y: 0.0,
                            velocity_z: 1.0,
                            animation_state: "Chase".to_string(),
                            health: enemy.health,
                            max_health: enemy.max_health,
                            attack_damage: enemy.attack_damage,
                            attack_range: enemy.attack_range,
                            attack_speed: enemy.attack_speed,
                            last_attack_time: enemy.last_attack_time,
                        });
                    }
                    black_box(&updates);
                })
            },
        );

        // New: copy u8/u32 fields — zero String allocations
        group.bench_with_input(
            BenchmarkId::new("new_u8", count),
            &enemies_new,
            |b, enemies| {
                b.iter(|| {
                    let mut updates = Vec::with_capacity(enemies.len());
                    for enemy in enemies {
                        updates.push(EnemyNew {
                            id: enemy.id,
                            enemy_type: enemy.enemy_type,
                            world_id: enemy.world_id,
                            x: enemy.x + 0.1,
                            y: enemy.y,
                            z: enemy.z + 0.1,
                            rotation_y: 0.5,
                            velocity_x: 1.0,
                            velocity_y: 0.0,
                            velocity_z: 1.0,
                            animation_state: EnemyBehaviorKind::CHASE,
                            health: enemy.health,
                            max_health: enemy.max_health,
                            attack_damage: enemy.attack_damage,
                            attack_range: enemy.attack_range,
                            attack_speed: enemy.attack_speed,
                            last_attack_time: enemy.last_attack_time,
                        });
                    }
                    black_box(&updates);
                })
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark: full tick simulation (grouping + grid + decisions + updates)
// Excludes physics step (avian3d) and actual DB I/O.
// ============================================================================

fn bench_full_tick_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_tick_no_physics");
    group.sample_size(50);

    for count in [1000, 5000, 10000] {
        // Old path
        group.bench_with_input(
            BenchmarkId::new("old_string", count),
            &count,
            |b, &count| {
                let enemies = make_enemies_old(count, "world1");
                let players = make_players(1, 1);
                let now = 1_000_000_000i64;
                let cooldown_micros = (defaults::ENEMY_ATTACK_COOLDOWN * 1_000_000.0) as i64;
                let sep_radius = defaults::ENEMY_SEPARATION_RADIUS;
                let inv_cell = 1.0 / sep_radius;

                b.iter(|| {
                    // 1. Group by world_id (old: clone every row)
                    let mut enemies_by_world: HashMap<String, Vec<&EnemyOld>> = HashMap::new();
                    for e in &enemies {
                        enemies_by_world
                            .entry(e.world_id.clone())
                            .or_default()
                            .push(e);
                    }

                    for (_world_id, enemies) in &enemies_by_world {
                        let players = &players;

                        // 2. Spatial grid
                        let mut grid: HashMap<(i32, i32), Vec<usize>> =
                            HashMap::with_capacity(enemies.len());
                        for (idx, enemy) in enemies.iter().enumerate() {
                            let cx = (enemy.x * inv_cell).floor() as i32;
                            let cz = (enemy.z * inv_cell).floor() as i32;
                            grid.entry((cx, cz)).or_default().push(idx);
                        }

                        // 3. AI decisions
                        let mut decisions = Vec::with_capacity(enemies.len());
                        for enemy in enemies {
                            let mut nearest_dist = f32::MAX;
                            for p in players {
                                let dx = p.x - enemy.x;
                                let dz = p.z - enemy.z;
                                let dist = (dx * dx + dz * dz).sqrt();
                                if dist < nearest_dist {
                                    nearest_dist = dist;
                                }
                            }
                            let ready = (now - enemy.last_attack_time) >= cooldown_micros;
                            decisions.push(enemy_ai_decision(nearest_dist, ready));
                        }

                        // 4. Build update structs (old: 3 String allocs per enemy)
                        let mut updates = Vec::with_capacity(enemies.len());
                        for (i, enemy) in enemies.iter().enumerate() {
                            updates.push(EnemyOld {
                                id: enemy.id,
                                enemy_type: enemy.enemy_type.clone(),
                                world_id: enemy.world_id.clone(),
                                x: enemy.x,
                                y: enemy.y,
                                z: enemy.z,
                                rotation_y: enemy.rotation_y,
                                velocity_x: 0.0,
                                velocity_y: 0.0,
                                velocity_z: 0.0,
                                animation_state: decisions[i].as_str().to_string(),
                                health: enemy.health,
                                max_health: enemy.max_health,
                                attack_damage: enemy.attack_damage,
                                attack_range: enemy.attack_range,
                                attack_speed: enemy.attack_speed,
                                last_attack_time: enemy.last_attack_time,
                            });
                        }
                        black_box(&updates);
                    }
                })
            },
        );

        // New path
        group.bench_with_input(BenchmarkId::new("new_u8", count), &count, |b, &count| {
            let enemies = make_enemies_new(count, 1);
            let players = make_players(1, 1);
            let now = 1_000_000_000i64;
            let cooldown_micros = (defaults::ENEMY_ATTACK_COOLDOWN * 1_000_000.0) as i64;
            let sep_radius = defaults::ENEMY_SEPARATION_RADIUS;
            let inv_cell = 1.0 / sep_radius;

            b.iter(|| {
                // 1. Group by world_id (new: u32 key — zero-cost copy)
                let mut enemies_by_world: HashMap<u32, Vec<&EnemyNew>> = HashMap::new();
                for e in &enemies {
                    enemies_by_world.entry(e.world_id).or_default().push(e);
                }
                let mut players_by_world: HashMap<u32, Vec<&Player>> = HashMap::new();
                for p in &players {
                    players_by_world.entry(p.world_id).or_default().push(p);
                }

                for (world_id, enemies) in &enemies_by_world {
                    let Some(players) = players_by_world.get(world_id) else {
                        continue;
                    };

                    // 2. Spatial grid (flat counting-sort)
                    // Convert &[&EnemyNew] to temporary slice for flat_grid_separation
                    let enemy_slice: Vec<EnemyNew> = enemies.iter().map(|e| (*e).clone()).collect();
                    let _separation = flat_grid_separation(
                        &enemy_slice,
                        inv_cell,
                        sep_radius * sep_radius,
                        defaults::ENEMY_SEPARATION_STRENGTH,
                    );

                    // 3. AI decisions
                    let mut decisions = Vec::with_capacity(enemies.len());
                    for enemy in enemies {
                        let mut nearest_dist = f32::MAX;
                        for p in players {
                            let dx = p.x - enemy.x;
                            let dz = p.z - enemy.z;
                            let dist = (dx * dx + dz * dz).sqrt();
                            if dist < nearest_dist {
                                nearest_dist = dist;
                            }
                        }
                        let ready = (now - enemy.last_attack_time) >= cooldown_micros;
                        decisions.push(enemy_ai_decision(nearest_dist, ready));
                    }

                    // 4. Build update structs (new: u8/u32 copy, no String alloc)
                    let mut updates = Vec::with_capacity(enemies.len());
                    for (i, enemy) in enemies.iter().enumerate() {
                        updates.push(EnemyNew {
                            id: enemy.id,
                            enemy_type: enemy.enemy_type,
                            world_id: enemy.world_id,
                            x: enemy.x,
                            y: enemy.y,
                            z: enemy.z,
                            rotation_y: enemy.rotation_y,
                            velocity_x: 0.0,
                            velocity_y: 0.0,
                            velocity_z: 0.0,
                            animation_state: decisions[i].as_u8(),
                            health: enemy.health,
                            max_health: enemy.max_health,
                            attack_damage: enemy.attack_damage,
                            attack_range: enemy.attack_range,
                            attack_speed: enemy.attack_speed,
                            last_attack_time: enemy.last_attack_time,
                        });
                    }
                    black_box(&updates);
                }
            })
        });
    }

    group.finish();
}

// ============================================================================
// Benchmark: world_id copy cost — with u32, this is trivial.
// Shows that the S2 skip-unchanged check is the real win.
// ============================================================================

fn bench_world_id_skip_unchanged(c: &mut Criterion) {
    let mut group = c.benchmark_group("world_id_skip_unchanged");

    for count in [1000, 5000, 10000] {
        let enemies = make_enemies_new(count, 1);

        // All enemies update (worst case: N copies — trivial with u32)
        group.bench_with_input(
            BenchmarkId::new("all_changed", count),
            &enemies,
            |b, enemies| {
                b.iter(|| {
                    let world_id = enemies[0].world_id;
                    let mut updates = Vec::with_capacity(enemies.len());
                    for enemy in enemies {
                        updates.push(EnemyNew {
                            id: enemy.id,
                            enemy_type: enemy.enemy_type,
                            world_id,
                            x: enemy.x + 0.1,
                            y: enemy.y,
                            z: enemy.z + 0.1,
                            rotation_y: 0.5,
                            velocity_x: 1.0,
                            velocity_y: 0.0,
                            velocity_z: 1.0,
                            animation_state: EnemyBehaviorKind::CHASE,
                            health: enemy.health,
                            max_health: enemy.max_health,
                            attack_damage: enemy.attack_damage,
                            attack_range: enemy.attack_range,
                            attack_speed: enemy.attack_speed,
                            last_attack_time: enemy.last_attack_time,
                        });
                    }
                    black_box(&updates);
                })
            },
        );

        // Only 10% of enemies changed (typical: most idle enemies don't move)
        group.bench_with_input(
            BenchmarkId::new("10pct_changed", count),
            &enemies,
            |b, enemies| {
                b.iter(|| {
                    let world_id = enemies[0].world_id;
                    let mut updates = Vec::with_capacity(enemies.len() / 10);
                    for (i, enemy) in enemies.iter().enumerate() {
                        // Simulate skip-unchanged: only 10% actually write
                        if i % 10 != 0 {
                            continue;
                        }
                        updates.push(EnemyNew {
                            id: enemy.id,
                            enemy_type: enemy.enemy_type,
                            world_id,
                            x: enemy.x + 0.1,
                            y: enemy.y,
                            z: enemy.z + 0.1,
                            rotation_y: 0.5,
                            velocity_x: 1.0,
                            velocity_y: 0.0,
                            velocity_z: 1.0,
                            animation_state: EnemyBehaviorKind::CHASE,
                            health: enemy.health,
                            max_health: enemy.max_health,
                            attack_damage: enemy.attack_damage,
                            attack_range: enemy.attack_range,
                            attack_speed: enemy.attack_speed,
                            last_attack_time: enemy.last_attack_time,
                        });
                    }
                    black_box(&updates);
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_grouping,
    bench_spatial_grid,
    bench_ai_decisions,
    bench_build_update,
    bench_world_id_skip_unchanged,
    bench_full_tick_simulation,
);
criterion_main!(benches);

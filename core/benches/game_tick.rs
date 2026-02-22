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
use game_core::combat::{defaults, enemy_ai_decision, EnemyBehaviorKind};
use std::collections::HashMap;

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
    world_id: String,
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

#[derive(Clone)]
struct Player {
    x: f32,
    z: f32,
    world_id: String,
}

// ============================================================================
// Data generation
// ============================================================================

fn make_players(count: usize, world_id: &str) -> Vec<Player> {
    (0..count)
        .map(|_| Player {
            x: 0.0,
            z: 0.0,
            world_id: world_id.to_string(),
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

fn make_enemies_new(count: usize, world_id: &str) -> Vec<EnemyNew> {
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
                world_id: world_id.to_string(),
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
        let enemies_new = make_enemies_new(count, "world1");

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

        // New approach: get_mut/insert only clones on first occurrence
        group.bench_with_input(
            BenchmarkId::new("new_get_mut", count),
            &enemies_new,
            |b, enemies| {
                b.iter(|| {
                    let mut by_world: HashMap<String, Vec<&EnemyNew>> = HashMap::new();
                    for e in enemies {
                        if let Some(vec) = by_world.get_mut(&e.world_id) {
                            vec.push(e);
                        } else {
                            by_world.insert(e.world_id.clone(), vec![e]);
                        }
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
        let enemies = make_enemies_new(count, "world1");

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
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
    }

    group.finish();
}

// ============================================================================
// Benchmark: AI decisions + velocity computation
// ============================================================================

fn bench_ai_decisions(c: &mut Criterion) {
    let mut group = c.benchmark_group("ai_decisions");

    for count in [1000, 5000, 10000] {
        let enemies = make_enemies_new(count, "world1");
        let players = make_players(1, "world1");
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
                        let decision = enemy_ai_decision(nearest_dist, ready);
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
        let enemies_new = make_enemies_new(count, "world1");

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

        // New: copy u8 fields + clone world_id only (no String alloc for anim/type)
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
                            world_id: enemy.world_id.clone(),
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
                let players = make_players(1, "world1");
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
                    let mut players_by_world: HashMap<String, Vec<&Player>> = HashMap::new();
                    for p in &players {
                        players_by_world
                            .entry(p.world_id.clone())
                            .or_default()
                            .push(p);
                    }

                    for (world_id, enemies) in &enemies_by_world {
                        let Some(players) = players_by_world.get(world_id) else {
                            continue;
                        };

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
        group.bench_with_input(
            BenchmarkId::new("new_u8", count),
            &count,
            |b, &count| {
                let enemies = make_enemies_new(count, "world1");
                let players = make_players(1, "world1");
                let now = 1_000_000_000i64;
                let cooldown_micros = (defaults::ENEMY_ATTACK_COOLDOWN * 1_000_000.0) as i64;
                let sep_radius = defaults::ENEMY_SEPARATION_RADIUS;
                let inv_cell = 1.0 / sep_radius;

                b.iter(|| {
                    // 1. Group by world_id (new: get_mut avoids clone)
                    let mut enemies_by_world: HashMap<String, Vec<&EnemyNew>> = HashMap::new();
                    for e in &enemies {
                        if let Some(vec) = enemies_by_world.get_mut(&e.world_id) {
                            vec.push(e);
                        } else {
                            enemies_by_world.insert(e.world_id.clone(), vec![e]);
                        }
                    }
                    let mut players_by_world: HashMap<String, Vec<&Player>> = HashMap::new();
                    for p in &players {
                        if let Some(vec) = players_by_world.get_mut(&p.world_id) {
                            vec.push(p);
                        } else {
                            players_by_world.insert(p.world_id.clone(), vec![p]);
                        }
                    }

                    for (world_id, enemies) in &enemies_by_world {
                        let Some(players) = players_by_world.get(world_id) else {
                            continue;
                        };

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

                        // 4. Build update structs (new: u8 copy, no String alloc)
                        let mut updates = Vec::with_capacity(enemies.len());
                        for (i, enemy) in enemies.iter().enumerate() {
                            updates.push(EnemyNew {
                                id: enemy.id,
                                enemy_type: enemy.enemy_type,
                                world_id: enemy.world_id.clone(),
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
    bench_full_tick_simulation,
);
criterion_main!(benches);

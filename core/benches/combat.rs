//! Criterion benchmarks for game-core combat and rules systems.
//!
//! Run: cargo bench -p game-core
//!
//! These establish baselines for the optimization targets identified in QA:
//! 1. Stat::Custom("Stacks") vs a hypothetical first-class Stat::Stacks variant
//! 2. HashMap<Stat,f32> lookup cost with String-keyed variants
//! 3. resolve_combat with varying target counts (linear search cost)
//! 4. Rule tree construction (default_player_rules) cost
//! 5. Full attack_hit–equivalent pipeline

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

use game_core::combat::{
    CombatInput, HitTarget, defaults, resolve_attack, resolve_combat, AttackInput,
};
use game_core::presets;
use game_core::rules::{Action, Stat, Stats, execute_rules};

// ============================================================================
// Helpers
// ============================================================================

fn default_attacker_stats(stacks: f32) -> Stats {
    Stats::new()
        .with(Stat::AttackDamage, defaults::ATTACK_DAMAGE)
        .with(Stat::CritChance, defaults::CRIT_CHANCE)
        .with(Stat::CritMultiplier, defaults::CRIT_MULTIPLIER)
        .with(Stat::Knockback, defaults::KNOCKBACK)
        .with(Stat::AttackRange, defaults::ATTACK_RANGE)
        .with(Stat::AttackArc, defaults::ATTACK_ARC)
        .with(Stat::Stacks, stacks)
        .with(Stat::AttackSpeed, 1.0)
}

fn make_targets(count: usize) -> Vec<HitTarget> {
    (0..count)
        .map(|i| {
            let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
            let dist = 1.5; // within attack range
            HitTarget {
                id: i as u64,
                pos: glam::Vec2::new(angle.cos() * dist, angle.sin() * dist),
                health: defaults::ENEMY_HEALTH,
            }
        })
        .collect()
}

// ============================================================================
// Benchmark 1: Stat variant HashMap lookup
// ============================================================================

fn bench_stat_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("stat_lookup");

    let stats = default_attacker_stats(5.0);

    // Lookup a built-in variant (no String hashing)
    group.bench_function("builtin_variant", |b| {
        b.iter(|| {
            black_box(stats.get(&Stat::AttackDamage));
        })
    });

    // Lookup Stat::Stacks (now a first-class variant, same cost as builtin)
    group.bench_function("stacks_variant", |b| {
        b.iter(|| {
            black_box(stats.get(&Stat::Stacks));
        })
    });

    // For reference: what Custom(String) used to cost (pre-allocated key)
    group.bench_function("custom_string_preallocated", |b| {
        let key = Stat::Custom("SomeOtherStat".into());
        let stats_with_custom = stats.clone().with(key.clone(), 1.0);
        b.iter(|| {
            black_box(stats_with_custom.get(&key));
        })
    });

    // For reference: what Custom(String) used to cost (alloc each time)
    group.bench_function("custom_string_alloc_each_time", |b| {
        let stats_with_custom = stats.clone().with(Stat::Custom("SomeOtherStat".into()), 1.0);
        b.iter(|| {
            black_box(stats_with_custom.get(&Stat::Custom("SomeOtherStat".into())));
        })
    });

    group.finish();
}

// ============================================================================
// Benchmark 2: Rule tree construction (default_player_rules)
// ============================================================================

fn bench_rule_construction(c: &mut Criterion) {
    c.bench_function("default_player_rules", |b| {
        b.iter(|| {
            black_box(presets::default_player_rules());
        })
    });
}

// ============================================================================
// Benchmark 3: resolve_attack (single target, pre-hit rules)
// ============================================================================

fn bench_resolve_attack(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolve_attack");

    let rules = presets::default_player_rules();

    group.bench_function("normal_hit", |b| {
        let stats = default_attacker_stats(0.0);
        b.iter(|| {
            black_box(resolve_attack(&AttackInput {
                attacker_stats: &stats,
                pre_hit_rules: &rules.pre_hit,
                rng_roll: 0.99, // no crit
            }));
        })
    });

    group.bench_function("crit_hit", |b| {
        let stats = default_attacker_stats(5.0);
        b.iter(|| {
            black_box(resolve_attack(&AttackInput {
                attacker_stats: &stats,
                pre_hit_rules: &rules.pre_hit,
                rng_roll: 0.01, // guaranteed crit
            }));
        })
    });

    group.finish();
}

// ============================================================================
// Benchmark 4: resolve_combat with varying target counts
// ============================================================================

fn bench_resolve_combat(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolve_combat");

    let rules = presets::default_player_rules();
    let stats = default_attacker_stats(3.0);
    let origin = glam::Vec2::ZERO;
    let forward = glam::Vec2::new(0.0, -1.0);
    let half_arc_cos = (defaults::ATTACK_ARC / 2.0_f32).to_radians().cos();

    for count in [1, 10, 50, 100, 500] {
        let targets = make_targets(count);
        group.bench_with_input(BenchmarkId::from_parameter(count), &targets, |b, targets| {
            b.iter(|| {
                black_box(resolve_combat(&CombatInput {
                    origin,
                    forward,
                    base_range: defaults::ATTACK_RANGE,
                    half_arc_cos,
                    attacker_stats: &stats,
                    rules: &rules,
                    rng_seed: 12345,
                    targets,
                }));
            })
        });
    }

    group.finish();
}

// ============================================================================
// Benchmark 5: On-hit stacking rule execution
// ============================================================================

fn bench_stacking_rules(c: &mut Criterion) {
    let mut group = c.benchmark_group("stacking_rules");

    let rules = presets::default_player_rules();

    group.bench_function("on_hit_10_stacks", |b| {
        let stats = default_attacker_stats(10.0);
        b.iter(|| {
            let mut s = stats.clone();
            let mut action = Action::new();
            black_box(execute_rules(&rules.on_hit, &mut s, &mut action));
        })
    });

    group.bench_function("on_crit_hit_10_stacks", |b| {
        let stats = default_attacker_stats(10.0);
        b.iter(|| {
            let mut s = stats.clone();
            let mut action = Action::new();
            black_box(execute_rules(&rules.on_crit_hit, &mut s, &mut action));
        })
    });

    group.finish();
}

// ============================================================================
// Benchmark 6: Full attack pipeline (simulates server attack_hit reducer)
// ============================================================================

fn bench_full_attack_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_attack_pipeline");

    for enemy_count in [10, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("enemies", enemy_count),
            &enemy_count,
            |b, &count| {
                b.iter(|| {
                    // 1. Build rules (currently rebuilt every attack)
                    let rules = presets::default_player_rules();

                    // 2. Build attacker stats
                    let stats = default_attacker_stats(5.0);

                    let origin = glam::Vec2::ZERO;
                    let forward = glam::Vec2::new(0.0, -1.0);
                    let half_arc_cos = (defaults::ATTACK_ARC / 2.0_f32).to_radians().cos();

                    // 3. Build target list
                    let targets = make_targets(count);

                    // 4. Resolve combat
                    let output = resolve_combat(&CombatInput {
                        origin,
                        forward,
                        base_range: defaults::ATTACK_RANGE,
                        half_arc_cos,
                        attacker_stats: &stats,
                        rules: &rules,
                        rng_seed: 42,
                        targets: &targets,
                    });

                    // 5. Linear search per hit (current pattern from combat.rs:107)
                    for hit in &output.hits {
                        black_box(targets.iter().find(|t| t.id == hit.target_id));
                    }

                    black_box(output);
                })
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 7: Stats clone cost (cloned per-target in resolve_combat)
// ============================================================================

fn bench_stats_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("stats_clone");

    let stats_small = Stats::new()
        .with(Stat::AttackDamage, 25.0)
        .with(Stat::CritChance, 0.2);

    let stats_full = default_attacker_stats(5.0);

    group.bench_function("2_entries", |b| {
        b.iter(|| black_box(stats_small.clone()))
    });

    group.bench_function("8_entries", |b| {
        b.iter(|| black_box(stats_full.clone()))
    });

    group.finish();
}

// ============================================================================

criterion_group!(
    benches,
    bench_stat_lookup,
    bench_rule_construction,
    bench_resolve_attack,
    bench_resolve_combat,
    bench_stacking_rules,
    bench_full_attack_pipeline,
    bench_stats_clone,
);
criterion_main!(benches);

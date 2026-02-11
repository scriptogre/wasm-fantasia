use spacetimedb::Table;
use wasm_fantasia_shared::combat::{self, defaults, resolve_combat, CombatInput, HitTarget};
use wasm_fantasia_shared::presets;
use wasm_fantasia_shared::rules::{Stat, Stats};

use crate::schema::*;

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

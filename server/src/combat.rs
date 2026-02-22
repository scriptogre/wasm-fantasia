use game_core::combat::{
    self, defaults, effect_types, knockback_displacement, resolve_combat, CombatInput, HitTarget,
};
use game_core::presets::{self, EntityRules};
use game_core::rules::{Stat, Stats};
use spacetimedb::Table;

use crate::schema::*;

thread_local! {
    static PLAYER_RULES: EntityRules = presets::default_player_rules();
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
    let stale_ids: Vec<u64> = ctx
        .db
        .combat_event()
        .iter()
        .filter(|e| e.world_id == attacker.world_id && e.timestamp < stale_threshold)
        .map(|e| e.id)
        .collect();
    for id in stale_ids {
        ctx.db.combat_event().id().delete(id);
    }

    // Cooldown check
    if !combat::can_attack(attacker.last_attack_time, now, attacker.attack_speed) {
        return;
    }

    // Read stacking buff from active_effect table (btree index on owner)
    let stacking_effect = ctx
        .db
        .active_effect()
        .owner()
        .filter(&ctx.sender)
        .find(|e| e.effect_type == effect_types::STACKING_DAMAGE);

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
        .with(Stat::Stacks, stacks)
        .with(Stat::AttackSpeed, effective_speed);

    // Build target list from enemies in the same world
    let enemy_targets: Vec<Enemy> = ctx
        .db
        .enemy()
        .iter()
        .filter(|e| e.health > 0.0 && e.world_id == attacker.world_id)
        .collect();

    // O(1) position lookup for combat events (avoids O(N) linear search per hit)
    let enemy_pos_index: std::collections::HashMap<u64, (f32, f32, f32)> = enemy_targets
        .iter()
        .map(|e| (e.id, (e.x, e.y, e.z)))
        .collect();

    let hit_targets: Vec<HitTarget> = enemy_targets
        .iter()
        .map(|e| HitTarget {
            id: e.id,
            pos: glam::Vec2::new(e.x, e.z),
            health: e.health,
        })
        .collect();

    let output = PLAYER_RULES.with(|rules| {
        resolve_combat(&CombatInput {
            origin,
            forward: fwd,
            base_range: attacker.attack_range,
            half_arc_cos,
            attacker_stats: &attacker_stats,
            rules,
            rng_seed: now as u64,
            targets: &hit_targets,
        })
    });

    let world_id = attacker.world_id;
    for hit in &output.hits {
        let (hit_x, hit_y, hit_z) = enemy_pos_index
            .get(&hit.target_id)
            .copied()
            .unwrap_or((attacker.x, attacker.y, attacker.z));

        ctx.db.combat_event().insert(CombatEvent {
            id: 0,
            x: hit_x,
            y: hit_y,
            z: hit_z,
            damage: hit.damage,
            is_crit: hit.is_crit,
            world_id,
            timestamp: now,
        });

        if let Some(enemy) = ctx.db.enemy().id().find(hit.target_id) {
            if hit.died {
                ctx.db.enemy().delete(enemy);
            } else {
                // Physics-based knockback: insert an impulse for the next game_tick
                let radial = glam::Vec2::new(enemy.x - attacker.x, enemy.z - attacker.z);
                let radial_dir = radial.normalize_or(fwd);
                let disp = combat::knockback_displacement(
                    radial_dir,
                    fwd,
                    hit.knockback,
                    hit.push,
                    hit.launch,
                );

                // Convert displacement to impulse (multiply by enemy mass)
                let enemy_mass = defaults::ENEMY_MASS;
                ctx.db.knockback_impulse().insert(KnockbackImpulse {
                    id: 0,
                    enemy_id: enemy.id,
                    world_id,
                    impulse_x: disp.x * enemy_mass,
                    impulse_y: disp.y * enemy_mass,
                    impulse_z: disp.z * enemy_mass,
                });

                ctx.db.enemy().id().update(Enemy {
                    health: hit.new_health,
                    ..enemy
                });
            }
        }
    }

    // Update attacker state
    let new_stacks = output.attacker_stats.get(&Stat::Stacks);
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
                effect_type: effect_types::STACKING_DAMAGE,
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

// ── Ground Pound AOE ─────────────────────────────────────────────

/// Server-authoritative ground pound AOE. Client sends impact position.
#[spacetimedb::reducer]
pub fn ground_pound_hit(ctx: &spacetimedb::ReducerContext, x: f32, y: f32, z: f32) {
    use combat::ground_pound as gp;

    let Some(attacker) = ctx.db.player().identity().find(ctx.sender) else {
        return;
    };
    if attacker.health <= 0.0 {
        return;
    }

    aoe_hit(
        ctx,
        &attacker,
        x,
        y,
        z,
        gp::RADIUS,
        gp::KNOCKBACK,
        gp::LAUNCH,
        gp::DAMAGE_MULTIPLIER,
    );
}

// ── Landing AOE ──────────────────────────────────────────────────

/// Server-authoritative landing AOE. Client sends velocity + impact position.
#[spacetimedb::reducer]
pub fn landing_aoe_hit(ctx: &spacetimedb::ReducerContext, velocity_y: f32, x: f32, y: f32, z: f32) {
    use combat::landing_aoe;

    let Some(attacker) = ctx.db.player().identity().find(ctx.sender) else {
        return;
    };
    if attacker.health <= 0.0 {
        return;
    }

    if velocity_y < landing_aoe::MIN_VELOCITY {
        return;
    }

    let (radius, kb, launch) = landing_aoe::scaled_params(velocity_y);
    aoe_hit(
        ctx,
        &attacker,
        x,
        y,
        z,
        radius,
        kb,
        launch,
        landing_aoe::DAMAGE_MULTIPLIER,
    );
}

// ── Shared AOE helper ────────────────────────────────────────────

fn aoe_hit(
    ctx: &spacetimedb::ReducerContext,
    attacker: &Player,
    impact_x: f32,
    impact_y: f32,
    impact_z: f32,
    radius: f32,
    kb: f32,
    launch: f32,
    damage_multiplier: f32,
) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();

    let base_damage = if attacker.attack_damage > 0.0 {
        attacker.attack_damage
    } else {
        defaults::ATTACK_DAMAGE
    };

    let attacker_stats = Stats::new()
        .with(Stat::AttackDamage, base_damage * damage_multiplier)
        .with(Stat::CritChance, attacker.crit_chance)
        .with(Stat::CritMultiplier, attacker.crit_multiplier)
        .with(Stat::Knockback, kb);

    let vertical_reach = defaults::ATTACK_VERTICAL_REACH * 2.0;

    let enemy_targets: Vec<Enemy> = ctx
        .db
        .enemy()
        .iter()
        .filter(|e| {
            if e.health <= 0.0 || e.world_id != attacker.world_id {
                return false;
            }
            let dx = e.x - impact_x;
            let dz = e.z - impact_z;
            let xz_dist = (dx * dx + dz * dz).sqrt();
            let vert_ok = (e.y - impact_y).abs() <= vertical_reach;
            xz_dist <= radius && vert_ok
        })
        .collect();

    if enemy_targets.is_empty() {
        return;
    }

    let hit_targets: Vec<HitTarget> = enemy_targets
        .iter()
        .map(|e| HitTarget {
            id: e.id,
            pos: glam::Vec2::new(e.x, e.z),
            health: e.health,
        })
        .collect();

    let origin_xz = glam::Vec2::new(impact_x, impact_z);
    let forward_xz = glam::Vec2::new(1.0, 0.0); // direction irrelevant for 360° AOE

    let output = PLAYER_RULES.with(|rules| {
        resolve_combat(&CombatInput {
            origin: origin_xz,
            forward: forward_xz,
            base_range: radius,
            half_arc_cos: -1.0, // Full 360° AOE
            attacker_stats: &attacker_stats,
            rules,
            rng_seed: now as u64,
            targets: &hit_targets,
        })
    });

    let enemy_mass = 50.0_f32;

    let world_id = attacker.world_id;
    for hit in &output.hits {
        let Some(enemy) = ctx.db.enemy().id().find(hit.target_id) else {
            continue;
        };

        ctx.db.combat_event().insert(CombatEvent {
            id: 0,
            x: enemy.x,
            y: enemy.y,
            z: enemy.z,
            damage: hit.damage,
            is_crit: hit.is_crit,
            world_id,
            timestamp: now,
        });

        if hit.died {
            ctx.db.enemy().delete(enemy);
        } else {
            let radial = glam::Vec2::new(enemy.x - impact_x, enemy.z - impact_z);
            let radial_dir = radial.normalize_or(forward_xz);
            let disp = knockback_displacement(radial_dir, radial_dir, kb, 0.0, launch);

            ctx.db.knockback_impulse().insert(KnockbackImpulse {
                id: 0,
                enemy_id: enemy.id,
                world_id,
                impulse_x: disp.x * enemy_mass,
                impulse_y: disp.y * enemy_mass,
                impulse_z: disp.z * enemy_mass,
            });

            ctx.db.enemy().id().update(Enemy {
                health: hit.new_health,
                ..enemy
            });
        }
    }
}

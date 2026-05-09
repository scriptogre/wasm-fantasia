use game_core::combat::{self, defaults, effect_types, landing_aoe};
use game_core::runtime::{Combatant, Effect, Intent};
use spacetimedb::Table;

use crate::schema::*;
use crate::scripting;

/// Simple RNG from a seed — produces a float in [0, 1).
fn rng_from_seed(seed: u64) -> f32 {
    // xorshift64 for a quick pseudo-random value
    let mut s = seed;
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    (s % 10_000) as f32 / 10_000.0
}

/// Server-authoritative attack resolution.
#[spacetimedb::reducer]
pub fn attack_hit(ctx: &spacetimedb::ReducerContext) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    let Some(attacker) = ctx.db.player().identity().find(ctx.sender()) else {
        return;
    };

    if attacker.health <= 0.0 {
        return;
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
        .filter(&ctx.sender())
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

    let fwd = glam::Vec2::new(-attacker.rotation_y.sin(), -attacker.rotation_y.cos());

    // Build source combatant for scripting
    let source = Combatant {
        id: 0, // player source ID
        pos_x: attacker.x,
        pos_y: attacker.y,
        pos_z: attacker.z,
        dir_x: fwd.x,
        dir_z: fwd.y,
        health: attacker.health,
        max_health: attacker.max_health,
        attack_damage: attacker.attack_damage,
        crit_chance: attacker.crit_chance,
        crit_multiplier: attacker.crit_multiplier,
        knockback_force: attacker.knockback_force,
        attack_range: attacker.attack_range,
        attack_arc: attacker.attack_arc,
        attack_speed: effective_speed,
        fury_stacks: stacks as i64,
        attack_speed_bonus: 0.0,
        cooldown_ready: true,
        speed: 0.0,
    };

    // Build target list from enemies in the same world (indexed lookup)
    let enemy_targets: Vec<Enemy> = ctx
        .db
        .enemy()
        .world_id()
        .filter(&attacker.world_id)
        .filter(|e| e.health > 0.0)
        .collect();

    let targets: Vec<Combatant> = enemy_targets
        .iter()
        .map(|e| Combatant {
            id: e.id,
            pos_x: e.x,
            pos_y: e.y,
            pos_z: e.z,
            dir_x: 0.0,
            dir_z: 1.0,
            health: e.health,
            max_health: e.max_health,
            attack_damage: e.attack_damage,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            knockback_force: 0.0,
            attack_range: e.attack_range,
            attack_arc: 360.0,
            attack_speed: e.attack_speed,
            fury_stacks: 0,
            attack_speed_bonus: 0.0,
            cooldown_ready: false,
            speed: 0.0,
        })
        .collect();

    // O(1) position lookup for combat events
    let enemy_pos_index: std::collections::HashMap<u64, (f32, f32, f32)> = enemy_targets
        .iter()
        .map(|e| (e.id, (e.x, e.y, e.z)))
        .collect();

    let rng_roll = rng_from_seed(now as u64);
    let (intents, effects) = scripting::run_melee_attack(source, targets, rng_roll);

    let world_id = attacker.world_id;
    let mut hit_any = false;
    let mut new_stacks = stacks;
    let mut new_speed_bonus = 0.0_f32;
    let mut buff_applied = false;

    process_combat_intents(
        ctx,
        &intents,
        &effects,
        &attacker,
        world_id,
        &enemy_pos_index,
        &fwd,
        now,
        &mut hit_any,
        &mut new_stacks,
        &mut new_speed_bonus,
        &mut buff_applied,
    );

    // Persist stacking buff to active_effect
    if new_stacks != stacks || buff_applied || stacking_effect.is_some() {
        if let Some(effect) = stacking_effect {
            if new_stacks > 0.0 {
                ctx.db.active_effect().id().update(ActiveEffect {
                    magnitude: new_stacks,
                    timestamp: if hit_any { now } else { last_hit_time },
                    ..effect
                });
            } else {
                ctx.db.active_effect().delete(effect);
            }
        } else if new_stacks > 0.0 {
            ctx.db.active_effect().insert(ActiveEffect {
                id: 0,
                owner: ctx.sender(),
                effect_type: effect_types::STACKING_DAMAGE,
                magnitude: new_stacks,
                duration: -1.0,
                timestamp: now,
            });
        }
    }

    let new_attack_speed = if new_speed_bonus > 0.0 {
        1.0 + new_speed_bonus
    } else if stacks <= 0.0 {
        1.0
    } else {
        attacker.attack_speed
    };

    ctx.db.player().identity().update(Player {
        last_attack_time: now,
        attack_speed: new_attack_speed,
        last_update: now,
        ..attacker
    });
}

// ── Ground Pound AOE ─────────────────────────────────────────────

/// Server-authoritative ground pound AOE. Client sends impact position.
#[spacetimedb::reducer]
pub fn ground_pound_hit(ctx: &spacetimedb::ReducerContext, x: f32, y: f32, z: f32) {
    use combat::ground_pound as gp;

    let Some(attacker) = ctx.db.player().identity().find(ctx.sender()) else {
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
    let Some(attacker) = ctx.db.player().identity().find(ctx.sender()) else {
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
    _kb: f32,
    _launch: f32,
    damage_multiplier: f32,
) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();

    let base_damage = if attacker.attack_damage > 0.0 {
        attacker.attack_damage
    } else {
        defaults::ATTACK_DAMAGE
    };

    let vertical_reach = defaults::ATTACK_VERTICAL_REACH * 2.0;

    let enemy_targets: Vec<Enemy> = ctx
        .db
        .enemy()
        .world_id()
        .filter(&attacker.world_id)
        .filter(|e| {
            if e.health <= 0.0 {
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

    // Build source combatant for ground pound — position at impact point
    let source = Combatant {
        id: 0,
        pos_x: impact_x,
        pos_y: impact_y,
        pos_z: impact_z,
        dir_x: 1.0,
        dir_z: 0.0,
        health: attacker.health,
        max_health: attacker.max_health,
        attack_damage: base_damage * damage_multiplier,
        crit_chance: attacker.crit_chance,
        crit_multiplier: attacker.crit_multiplier,
        knockback_force: attacker.knockback_force,
        attack_range: radius,
        attack_arc: 360.0,
        attack_speed: attacker.attack_speed,
        fury_stacks: 0,
        attack_speed_bonus: 0.0,
        cooldown_ready: true,
        speed: 0.0,
    };

    let targets: Vec<Combatant> = enemy_targets
        .iter()
        .map(|e| Combatant {
            id: e.id,
            pos_x: e.x,
            pos_y: e.y,
            pos_z: e.z,
            dir_x: 0.0,
            dir_z: 1.0,
            health: e.health,
            max_health: e.max_health,
            attack_damage: e.attack_damage,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            knockback_force: 0.0,
            attack_range: e.attack_range,
            attack_arc: 360.0,
            attack_speed: e.attack_speed,
            fury_stacks: 0,
            attack_speed_bonus: 0.0,
            cooldown_ready: false,
            speed: 0.0,
        })
        .collect();

    let enemy_pos_index: std::collections::HashMap<u64, (f32, f32, f32)> = enemy_targets
        .iter()
        .map(|e| (e.id, (e.x, e.y, e.z)))
        .collect();

    let rng_roll = rng_from_seed(now as u64);
    let (intents, effects) = scripting::run_ground_pound(source, targets, rng_roll);

    let world_id = attacker.world_id;
    let fwd = glam::Vec2::new(1.0, 0.0); // direction irrelevant for 360deg AOE
    let mut hit_any = false;
    let mut new_stacks = 0.0_f32;
    let mut new_speed_bonus = 0.0_f32;
    let mut buff_applied = false;

    process_combat_intents(
        ctx,
        &intents,
        &effects,
        attacker,
        world_id,
        &enemy_pos_index,
        &fwd,
        now,
        &mut hit_any,
        &mut new_stacks,
        &mut new_speed_bonus,
        &mut buff_applied,
    );
}

// ── Intent/Effect processing ─────────────────────────────────────

/// Process Rune script intents and effects, applying them to SpacetimeDB tables.
#[allow(clippy::too_many_arguments)]
fn process_combat_intents(
    ctx: &spacetimedb::ReducerContext,
    intents: &[Intent],
    effects: &[Effect],
    attacker: &Player,
    world_id: u32,
    enemy_pos_index: &std::collections::HashMap<u64, (f32, f32, f32)>,
    fwd: &glam::Vec2,
    now: i64,
    hit_any: &mut bool,
    new_stacks: &mut f32,
    new_speed_bonus: &mut f32,
    buff_applied: &mut bool,
) {
    // Accumulate damage per target so we can batch health updates
    let mut damage_by_target: std::collections::HashMap<u64, f32> =
        std::collections::HashMap::new();
    let mut knockback_by_target: std::collections::HashMap<u64, f32> =
        std::collections::HashMap::new();

    for intent in intents {
        match intent {
            Intent::DamageDealt { target_id, amount } => {
                *damage_by_target.entry(*target_id).or_insert(0.0) += amount;
                *hit_any = true;
            }
            Intent::KnockbackApplied { target_id, force } => {
                *knockback_by_target.entry(*target_id).or_insert(0.0) += force;
            }
            Intent::StatSet { stat, value, .. } => {
                if stat == "fury_stacks" {
                    *new_stacks = *value;
                } else if stat == "attack_speed_bonus" {
                    *new_speed_bonus = *value;
                }
            }
            Intent::BuffAdded { .. } => {
                *buff_applied = true;
            }
            _ => {}
        }
    }

    // Build a set of crit target IDs from effects (crit_particles VFX)
    let crit_targets: std::collections::HashSet<u64> = effects
        .iter()
        .filter_map(|e| match e {
            Effect::Vfx { name, target_id } if name == "crit_particles" => Some(*target_id),
            _ => None,
        })
        .collect();

    let enemy_mass = defaults::ENEMY_MASS;

    // Apply damage and knockback to enemies
    for (target_id, total_damage) in &damage_by_target {
        let is_crit = crit_targets.contains(target_id);

        let (hit_x, hit_y, hit_z) = enemy_pos_index
            .get(target_id)
            .copied()
            .unwrap_or((attacker.x, attacker.y, attacker.z));

        ctx.db.combat_event().insert(CombatEvent {
            id: 0,
            x: hit_x,
            y: hit_y,
            z: hit_z,
            damage: *total_damage,
            is_crit,
            world_id,
            timestamp: now,
        });

        if let Some(enemy) = ctx.db.enemy().id().find(*target_id) {
            let new_health = (enemy.health - total_damage).max(0.0);
            let died = new_health <= 0.0;

            if died {
                ctx.db.enemy().delete(enemy);
            } else {
                // Apply knockback if present
                if let Some(&kb_force) = knockback_by_target.get(target_id) {
                    let radial = glam::Vec2::new(enemy.x - attacker.x, enemy.z - attacker.z);
                    let radial_dir = radial.normalize_or(*fwd);
                    let disp =
                        combat::knockback_displacement(radial_dir, *fwd, kb_force, 0.0, 0.0);

                    ctx.db.knockback_impulse().insert(KnockbackImpulse {
                        id: 0,
                        enemy_id: enemy.id,
                        world_id,
                        impulse_x: disp.x * enemy_mass,
                        impulse_y: disp.y * enemy_mass,
                        impulse_z: disp.z * enemy_mass,
                    });
                }

                ctx.db.enemy().id().update(Enemy {
                    health: new_health,
                    ..enemy
                });
            }
        }
    }
}

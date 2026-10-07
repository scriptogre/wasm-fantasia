use game_core::combat::{self, defaults, effect_types, landing_aoe};
use game_core::fury::{self, Fury};
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

    let mut fury = read_fury(ctx, &attacker, now);
    let speed = defaults::ATTACK_SPEED + fury::bonus_percent(fury.stacks) as f32 / 100.0;
    if !combat::can_attack(attacker.last_attack_time, now, speed) {
        return;
    }

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
        attack_speed: speed,
        fury_stacks: fury.stacks,
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
    let previous_fury = fury;
    process_combat_intents(
        ctx,
        &intents,
        &effects,
        &attacker,
        world_id,
        &enemy_pos_index,
        &fwd,
        now,
        &mut fury,
    );

    if fury != previous_fury {
        save_fury(ctx, &attacker, fury, now);
    }

    ctx.db.player().identity().update(Player {
        last_attack_time: now,
        attack_speed: defaults::ATTACK_SPEED + fury::bonus_percent(fury.stacks) as f32 / 100.0,
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
    let mut fury = read_fury(ctx, attacker, now);

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
        fury_stacks: fury.stacks,
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
    let previous_fury = fury;
    process_combat_intents(
        ctx,
        &intents,
        &effects,
        attacker,
        world_id,
        &enemy_pos_index,
        &fwd,
        now,
        &mut fury,
    );
    if fury != previous_fury {
        save_fury(ctx, attacker, fury, now);
    }
    let Some(player) = ctx.db.player().identity().find(attacker.identity) else {
        return;
    };
    ctx.db.player().identity().update(Player {
        attack_speed: defaults::ATTACK_SPEED + fury::bonus_percent(fury.stacks) as f32 / 100.0,
        ..player
    });
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
    fury: &mut Fury,
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
            }
            Intent::KnockbackApplied { target_id, force } => {
                *knockback_by_target.entry(*target_id).or_insert(0.0) += force;
            }
            Intent::FuryHit { entity_id: 0, is_crit } => {
                *fury = fury::on_hit(fury.stacks, *is_crit);
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

fn read_fury(ctx: &spacetimedb::ReducerContext, player: &Player, now: i64) -> Fury {
    ctx.db
        .active_effect()
        .owner()
        .filter(player.identity)
        .find(|e| e.effect_type == effect_types::STACKING_DAMAGE)
        .map(|e| {
            fury::elapse(
                Fury {
                    stacks: e.magnitude as i64,
                    remaining_micros: (e.duration as f64 * 1_000_000.0) as u64,
                },
                now.saturating_sub(e.timestamp).max(0) as u64,
            )
        })
        .unwrap_or_default()
}

fn save_fury(ctx: &spacetimedb::ReducerContext, player: &Player, fury: Fury, now: i64) {
    let existing = ctx
        .db
        .active_effect()
        .owner()
        .filter(player.identity)
        .find(|e| e.effect_type == effect_types::STACKING_DAMAGE);
    if fury.stacks == 0 {
        if let Some(effect) = existing {
            ctx.db.active_effect().delete(effect);
        }
        return;
    }
    let effect = ActiveEffect {
        id: existing.as_ref().map_or(0, |e| e.id),
        owner: player.identity,
        effect_type: effect_types::STACKING_DAMAGE,
        magnitude: fury.stacks as f32,
        duration: fury.remaining_micros as f32 / 1_000_000.0,
        timestamp: now,
    };
    if existing.is_some() {
        ctx.db.active_effect().id().update(effect);
    } else {
        ctx.db.active_effect().insert(effect);
    }
}

pub fn expire_fury(ctx: &spacetimedb::ReducerContext) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    for effect in ctx
        .db
        .active_effect()
        .iter()
        .filter(|e| e.effect_type == effect_types::STACKING_DAMAGE)
    {
        let Some(player) = ctx.db.player().identity().find(effect.owner) else {
            continue;
        };
        if read_fury(ctx, &player, now).stacks == 0 {
            ctx.db.active_effect().delete(effect);
            ctx.db.player().identity().update(Player {
                attack_speed: defaults::ATTACK_SPEED,
                ..player
            });
        }
    }
}

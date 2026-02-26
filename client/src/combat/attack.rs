use super::*;
use crate::player::ControlScheme;
use crate::player::control::{GroundPoundImpact, GroundPoundState, InputBuffer};
use crate::models::combat::{Stat, Stats};
use crate::scripting::{ActiveAbility, EntityBehaviors, ScriptRegistryRes};
use bevy_enhanced_input::prelude::{Fire, Start};
use bevy_tnua::prelude::TnuaController;
use game_core::combat::{HitFeedback, defaults, ground_pound};
use game_core::scripting::Command as ScriptCommand;
use game_core::scripting::types::Combatant as ScriptCombatant;
use std::collections::HashMap;

/// Visual constants for attack effects
pub const VFX_RANGE: f32 = 2.0;
pub const VFX_ARC_DEGREES: f32 = 120.0;

pub fn plugin(app: &mut App) {
    app.add_observer(handle_attack)
        .add_observer(handle_airborne_attack)
        .add_observer(on_attack_hit)
        .add_observer(on_ground_pound_hit)
        .add_systems(
            Update,
            (tick_attack_state, process_buffered_attack).run_if(in_state(Screen::Gameplay)),
        );
}

/// Grounded melee attack — fires continuously while held (`Fire`).
fn handle_attack(
    on: On<Fire<Attack>>,
    mut buffer: ResMut<InputBuffer>,
    mut query: Query<(&mut AttackState, &TnuaController<ControlScheme>), With<PlayerCombatant>>,
) {
    let Ok((mut attack_state, controller)) = query.get_mut(on.context) else {
        return;
    };

    let grounded = controller.basis_memory.standing_on_entity().is_some();
    if !grounded {
        return;
    }

    if attack_state.can_attack() {
        attack_state.start_attack(false);
    } else {
        buffer.buffer_attack();
    }
}

/// Airborne attack → ground pound. Only triggers on fresh press (`Start`),
/// so holding attack on the ground won't instantly ground pound when you jump.
fn handle_airborne_attack(
    on: On<Start<Attack>>,
    mut commands: Commands,
    query: Query<(&TnuaController<ControlScheme>, Has<GroundPoundState>), With<PlayerCombatant>>,
) {
    let Ok((controller, already_pounding)) = query.get(on.context) else {
        return;
    };

    let grounded = controller.basis_memory.standing_on_entity().is_some();
    if grounded || already_pounding {
        return;
    }

    commands.entity(on.context).try_insert(GroundPoundState);
}

/// Execute buffered attack when possible
fn process_buffered_attack(
    mut buffer: ResMut<InputBuffer>,
    mut query: Query<&mut AttackState, With<PlayerCombatant>>,
) {
    if buffer.attack.is_none() {
        return;
    }

    let Ok(mut attack_state) = query.single_mut() else {
        return;
    };

    if attack_state.can_attack() {
        buffer.attack = None;
        attack_state.start_attack(false);
    }
}

/// Tick attack state timers and trigger hits based on time (not animation events).
fn tick_attack_state(
    time: Res<Time>,
    mut query: Query<(Entity, &mut AttackState, Option<&Stats>)>,
    mut commands: Commands,
) {
    for (entity, mut state, stats) in query.iter_mut() {
        let speed_mult = stats
            .map(|s| {
                let speed = s.get(&Stat::AttackSpeed);
                if speed == 0.0 { 1.0 } else { speed }
            })
            .unwrap_or(1.0)
            .max(0.1);

        let scaled_delta = time.delta().mul_f32(speed_mult);
        state.cooldown.tick(scaled_delta);

        let dt = time.delta_secs() * speed_mult;

        match &mut state.phase {
            AttackPhase::Windup {
                elapsed,
                total_duration,
                hit_time,
            } => {
                *elapsed += dt;

                if *elapsed >= *hit_time {
                    commands.trigger(AttackIntent { attacker: entity });
                    let remaining_duration = *total_duration - *hit_time;
                    let overshoot = *elapsed - *hit_time;
                    state.phase = AttackPhase::Recovery {
                        elapsed: overshoot,
                        remaining_duration,
                        total_duration: *total_duration,
                    };
                }
            }
            AttackPhase::Recovery {
                elapsed,
                remaining_duration,
                ..
            } => {
                *elapsed += dt;
                if *elapsed >= *remaining_duration {
                    state.phase = AttackPhase::Ready;
                    state.is_crit = false;
                }
            }
            AttackPhase::Ready => {}
        }
    }
}

// ── Script helpers ───────────────────────────────────────────────

/// Build a `ScriptCombatant` from ECS state.
fn build_combatant(
    entity: Entity,
    transform: &Transform,
    health: &Health,
    stats: Option<&Stats>,
) -> ScriptCombatant {
    let pos = transform.translation;
    let fwd = transform.forward().as_vec3();
    let default_stats = Stats::default();
    let s = stats.unwrap_or(&default_stats);
    ScriptCombatant {
        id: entity.to_bits(),
        pos_x: pos.x,
        pos_y: pos.y,
        pos_z: pos.z,
        dir_x: fwd.x,
        dir_z: fwd.z,
        health: health.current,
        max_health: health.max,
        attack_damage: s.get(&Stat::AttackDamage).max(defaults::ATTACK_DAMAGE),
        crit_chance: s.get(&Stat::CritChance).max(defaults::CRIT_CHANCE),
        crit_multiplier: s.get(&Stat::CritMultiplier).max(defaults::CRIT_MULTIPLIER),
        knockback_force: s.get(&Stat::Knockback).max(defaults::KNOCKBACK),
        attack_range: s.get(&Stat::AttackRange).max(defaults::ATTACK_RANGE),
        attack_arc: s.get(&Stat::AttackArc).max(defaults::ATTACK_ARC),
        attack_speed: s.get(&Stat::AttackSpeed).max(defaults::ATTACK_SPEED),
        fury_stacks: s.get(&Stat::Stacks) as i64,
        attack_speed_bonus: 0.0,
        cooldown_ready: true,
        speed: 0.0,
    }
}

/// Map a stat name string from Rune scripts to the `Stat` enum.
fn stat_from_name(name: &str) -> Stat {
    match name {
        "fury_stacks" => Stat::Stacks,
        "attack_damage" => Stat::AttackDamage,
        "attack_speed" => Stat::AttackSpeed,
        "crit_chance" => Stat::CritChance,
        "crit_multiplier" => Stat::CritMultiplier,
        "knockback" => Stat::Knockback,
        "attack_range" => Stat::AttackRange,
        "attack_arc" => Stat::AttackArc,
        "health" => Stat::Health,
        "max_health" => Stat::MaxHealth,
        _ => Stat::Custom(name.to_string()),
    }
}

/// Process script commands into Bevy events and stat updates.
///
/// Groups `DealDamage` + `ApplyKnockback` per target into single `DamageDealt`
/// events. Applies `SetStat` commands to the attacker's `Stats` component.
/// Returns whether any crit occurred (for attack animation state).
fn process_script_commands(
    cmds: &[ScriptCommand],
    attacker_entity: Entity,
    origin_pos: Vec3,
    forward: Vec3,
    targets: &Query<(Entity, &Transform, &Health), With<Enemy>>,
    stats: &mut Option<Mut<'_, Stats>>,
    bevy_commands: &mut Commands,
) -> bool {
    let mut any_crit = false;
    let mut target_hits: HashMap<u64, (f32, f32)> = HashMap::new();

    for cmd in cmds {
        match cmd {
            ScriptCommand::DealDamage { target_id, amount } => {
                let entry = target_hits.entry(*target_id).or_insert((0.0, 0.0));
                entry.0 += amount;
            }
            ScriptCommand::ApplyKnockback { target_id, force } => {
                let entry = target_hits.entry(*target_id).or_insert((0.0, 0.0));
                entry.1 = *force;
            }
            ScriptCommand::SetStat { stat, value, .. } => {
                if let Some(s) = stats {
                    s.set(stat_from_name(stat), *value);
                }
            }
            ScriptCommand::AddBuff { .. } => {
                // Stacking script already emits SetStat alongside AddBuff;
                // buff duration tracking will come in a later task.
            }
            // Feedback (screen shake, hit stop, sound, VFX) is handled by
            // the existing Bevy feedback pipeline via HitLanded events.
            _ => {}
        }
    }

    let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let fwd_2d = Vec2::new(forward_flat.x, forward_flat.z);

    for (target_id, (damage, knockback_force)) in &target_hits {
        let target_entity = Entity::from_bits(*target_id);

        let Ok((_, target_tf, _)) = targets.get(target_entity) else {
            continue;
        };

        let is_crit = cmds.iter().any(|c| {
            matches!(c, ScriptCommand::SpawnVfx { name, target_id: tid }
                if name == "crit_particles" && tid == target_id)
        });
        if is_crit {
            any_crit = true;
        }

        let target_pos = target_tf.translation;
        let to_target = target_pos - origin_pos;
        let radial_2d = Vec2::new(to_target.x, to_target.z);
        let radial_dir = radial_2d.normalize_or(fwd_2d);

        let force = game_core::combat::knockback_displacement(
            radial_dir,
            fwd_2d,
            *knockback_force,
            0.0,
            0.0,
        );

        bevy_commands.trigger(DamageDealt {
            source: attacker_entity,
            target: target_entity,
            damage: *damage,
            force,
            is_crit,
            feedback: HitFeedback::standard(is_crit),
        });
    }

    any_crit
}

// ── Melee Attack ─────────────────────────────────────────────────

/// Observer: triggered when attack hit time is reached.
/// Calls the Rune ability script with behavior hooks and fires [`DamageDealt`] per hit.
fn on_attack_hit(
    trigger: On<AttackIntent>,
    mut attackers: Query<
        (
            &mut AttackState,
            &Transform,
            &Health,
            Option<&mut Stats>,
            Option<&EntityBehaviors>,
            Option<&ActiveAbility>,
        ),
        With<PlayerCombatant>,
    >,
    targets: Query<(Entity, &Transform, &Health), With<Enemy>>,
    registry: Option<Res<ScriptRegistryRes>>,
    mut commands: Commands,
) {
    let attacker_entity = trigger.event().attacker;
    let Ok((mut attack_state, transform, health, stats, behaviors, active_ability)) =
        attackers.get_mut(attacker_entity)
    else {
        return;
    };

    let Some(registry) = registry else {
        return;
    };

    let ability_name = active_ability
        .map(|a| a.0.as_str())
        .unwrap_or("melee_attack");

    let Some(ability_engine) = registry.0.get(ability_name) else {
        warn!("No script found for ability '{ability_name}'");
        return;
    };

    let attacker_pos = transform.translation;
    let forward = transform.forward().as_vec3();
    let attacker_stats = stats.as_ref().map(|s| &**s);
    let source = build_combatant(attacker_entity, transform, health, attacker_stats);

    // Build target list, filtering out targets too far above/below
    let vertical_reach = defaults::ATTACK_VERTICAL_REACH;
    let script_targets: Vec<ScriptCombatant> = targets
        .iter()
        .filter(|(_, tf, _)| (tf.translation.y - attacker_pos.y).abs() <= vertical_reach)
        .map(|(e, tf, h)| build_combatant(e, tf, h, None))
        .collect();

    let behavior_names = behaviors.map(|b| b.0.clone()).unwrap_or_default();

    let script_cmds: Vec<ScriptCommand> = match ability_engine.call_ability_with_behaviors(
        "on_ability_start",
        source,
        script_targets,
        rand::random(),
        registry.0.clone(),
        behavior_names,
    ) {
        Ok(cmds) => cmds,
        Err(e) => {
            warn!("Ability script '{ability_name}' failed: {e}");
            return;
        }
    };

    let mut stats_mut = stats;
    let any_crit = process_script_commands(
        &script_cmds,
        attacker_entity,
        attacker_pos,
        forward,
        &targets,
        &mut stats_mut,
        &mut commands,
    );

    attack_state.is_crit = any_crit;
}

// ── Ground Pound AOE ─────────────────────────────────────────────

/// Observer: ground pound landed — runs the ground_pound Rune script for AOE damage.
fn on_ground_pound_hit(
    trigger: On<GroundPoundImpact>,
    attackers: Query<
        (
            Entity,
            &Transform,
            &Health,
            Option<&Stats>,
            Option<&EntityBehaviors>,
        ),
        With<PlayerCombatant>,
    >,
    targets: Query<(Entity, &Transform, &Health), With<Enemy>>,
    registry: Option<Res<ScriptRegistryRes>>,
    mut commands: Commands,
) {
    let Ok((attacker_entity, transform, health, stats, behaviors)) = attackers.single() else {
        return;
    };

    let Some(registry) = registry else {
        return;
    };

    let Some(ability_engine) = registry.0.get("ground_pound") else {
        warn!("No script found for ability 'ground_pound'");
        return;
    };

    let impact_pos = trigger.event().position;
    let forward = transform.forward().as_vec3();

    // Build source combatant at the impact position
    let mut source = build_combatant(attacker_entity, transform, health, stats);
    source.pos_x = impact_pos.x;
    source.pos_y = impact_pos.y;
    source.pos_z = impact_pos.z;

    // Gather all enemies as potential targets (the script handles radius filtering)
    let vertical_reach = defaults::ATTACK_VERTICAL_REACH;
    let script_targets: Vec<ScriptCombatant> = targets
        .iter()
        .filter(|(_, tf, _)| (tf.translation.y - impact_pos.y).abs() <= vertical_reach)
        .map(|(e, tf, h)| build_combatant(e, tf, h, None))
        .collect();

    if script_targets.is_empty() {
        return;
    }

    let behavior_names = behaviors.map(|b| b.0.clone()).unwrap_or_default();

    let script_cmds: Vec<ScriptCommand> = match ability_engine.call_ability_with_behaviors(
        "on_ability_start",
        source,
        script_targets,
        rand::random(),
        registry.0.clone(),
        behavior_names,
    ) {
        Ok(cmds) => cmds,
        Err(e) => {
            warn!("Ground pound script failed: {e}");
            return;
        }
    };

    // For ground pound, knockback is radial from impact center
    let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let forward_xz = Vec2::new(forward_flat.x, forward_flat.z);

    let mut target_hits: HashMap<u64, (f32, f32)> = HashMap::new();

    for cmd in &script_cmds {
        match cmd {
            ScriptCommand::DealDamage { target_id, amount } => {
                let entry = target_hits.entry(*target_id).or_insert((0.0, 0.0));
                entry.0 += amount;
            }
            ScriptCommand::ApplyKnockback { target_id, force } => {
                let entry = target_hits.entry(*target_id).or_insert((0.0, 0.0));
                entry.1 = *force;
            }
            _ => {}
        }
    }

    for (target_id, (damage, knockback_force)) in &target_hits {
        let target_entity = Entity::from_bits(*target_id);

        let Ok((_, target_tf, _)) = targets.get(target_entity) else {
            continue;
        };

        let is_crit = script_cmds.iter().any(|c| {
            matches!(c, ScriptCommand::SpawnVfx { name, target_id: tid }
                if name == "crit_particles" && tid == target_id)
        });

        let target_pos = target_tf.translation;
        let to_target = target_pos - impact_pos;
        let radial_2d = Vec2::new(to_target.x, to_target.z);
        let radial_dir = radial_2d.normalize_or(forward_xz);

        let force = game_core::combat::knockback_displacement(
            radial_dir,
            radial_dir, // push direction = radial (outward from center)
            *knockback_force,
            0.0,
            ground_pound::LAUNCH,
        );

        commands.trigger(DamageDealt {
            source: attacker_entity,
            target: target_entity,
            damage: *damage,
            force,
            is_crit,
            feedback: HitFeedback::standard(is_crit),
        });
    }
}

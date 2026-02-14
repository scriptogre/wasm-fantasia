use super::*;
use crate::player::control::InputBuffer;
use crate::player::{ControlScheme, ControlSchemeActionDiscriminant};
use crate::rules::{
    OnCritHitRules, OnHitRules, OnKillRules, OnPreHitRules, OnTakeDamageRules, OnTickRules, Stat,
    Stats,
};
use bevy_enhanced_input::prelude::Fire;
use bevy_tnua::prelude::*;
use wasm_fantasia_shared::combat::{CombatInput, HitTarget, defaults, resolve_combat};
use wasm_fantasia_shared::presets::EntityRules;

/// Visual constants for attack effects
pub const VFX_RANGE: f32 = 2.0;
pub const VFX_ARC_DEGREES: f32 = 120.0;

pub fn plugin(app: &mut App) {
    app.add_observer(handle_attack)
        .add_observer(on_attack_hit)
        .add_systems(
            Update,
            (tick_attack_state, process_buffered_attack).run_if(in_state(Screen::Gameplay)),
        );
}

fn handle_attack(
    on: On<Fire<Attack>>,
    mut buffer: ResMut<InputBuffer>,
    mut query: Query<(&mut AttackState, &TnuaController<ControlScheme>), With<PlayerCombatant>>,
) {
    let Ok((mut attack_state, controller)) = query.get_mut(on.context) else {
        return;
    };

    // Block attacks during dash/slide - buffer for after dash
    if controller.action_discriminant() == Some(ControlSchemeActionDiscriminant::Dash) {
        buffer.buffer_attack();
        return;
    }

    if attack_state.can_attack() {
        attack_state.start_attack(false);
    } else {
        buffer.buffer_attack();
    }
}

/// Execute buffered attack when possible
fn process_buffered_attack(
    mut buffer: ResMut<InputBuffer>,
    mut query: Query<(&mut AttackState, &TnuaController<ControlScheme>), With<PlayerCombatant>>,
) {
    if buffer.attack.is_none() {
        return;
    }

    let Ok((mut attack_state, controller)) = query.single_mut() else {
        return;
    };

    if controller.action_discriminant() == Some(ControlSchemeActionDiscriminant::Dash) {
        return;
    }

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

/// Observer: triggered when attack hit time is reached.
/// Calls [`resolve_combat`] and fires [`DamageDealt`] per hit.
fn on_attack_hit(
    trigger: On<AttackIntent>,
    mut attackers: Query<
        (
            &mut AttackState,
            &Transform,
            Option<&mut Stats>,
            Option<&OnPreHitRules>,
            Option<&OnHitRules>,
            Option<&OnCritHitRules>,
            Option<&OnKillRules>,
            Option<&OnTakeDamageRules>,
            Option<&OnTickRules>,
        ),
        With<PlayerCombatant>,
    >,
    targets: Query<(Entity, &Transform, &Health), With<Enemy>>,
    mut commands: Commands,
) {
    let attacker_entity = trigger.event().attacker;
    let Ok((
        mut attack_state,
        transform,
        stats,
        pre_hit,
        on_hit,
        on_crit_hit,
        on_kill,
        on_take_damage,
        on_tick,
    )) = attackers.get_mut(attacker_entity)
    else {
        return;
    };

    let attacker_stats = stats.as_ref().map(|s| s.0.clone()).unwrap_or_default();

    let base_range = {
        let v = attacker_stats.get(&Stat::AttackRange);
        if v > 0.0 { v } else { defaults::ATTACK_RANGE }
    };
    let base_arc = {
        let v = attacker_stats.get(&Stat::AttackArc);
        if v > 0.0 { v } else { defaults::ATTACK_ARC }
    };

    let rules = EntityRules {
        pre_hit: pre_hit.map(|r| r.0.clone()).unwrap_or_default(),
        on_hit: on_hit.map(|r| r.0.clone()).unwrap_or_default(),
        on_crit_hit: on_crit_hit.map(|r| r.0.clone()).unwrap_or_default(),
        on_kill: on_kill.map(|r| r.0.clone()).unwrap_or_default(),
        on_take_damage: on_take_damage.map(|r| r.0.clone()).unwrap_or_default(),
        on_tick: on_tick.map(|r| r.0.clone()).unwrap_or_default(),
    };

    let attacker_pos = transform.translation;

    // Build target list with entity mapping, filtering out targets too far
    // above or below the attacker (the cone check is 2D on the XZ plane).
    let vertical_reach = defaults::ATTACK_VERTICAL_REACH;
    let target_list: Vec<(Entity, Vec3)> = targets
        .iter()
        .filter(|(_, tf, _)| (tf.translation.y - attacker_pos.y).abs() <= vertical_reach)
        .map(|(e, tf, _)| (e, tf.translation))
        .collect();
    let hit_targets: Vec<HitTarget> = target_list
        .iter()
        .map(|&(e, pos)| HitTarget {
            id: e.to_bits(),
            pos: Vec2::new(pos.x, pos.z),
            health: targets.get(e).map(|(_, _, h)| h.current).unwrap_or(0.0),
        })
        .collect();
    let forward = transform.forward().as_vec3();
    let forward_xz = Vec2::new(forward.x, forward.z).normalize_or_zero();
    let origin_xz = Vec2::new(attacker_pos.x, attacker_pos.z);
    let half_arc_cos = (base_arc / 2.0_f32).to_radians().cos();

    let output = resolve_combat(&CombatInput {
        origin: origin_xz,
        forward: forward_xz,
        base_range,
        half_arc_cos,
        attacker_stats: &attacker_stats,
        rules: &rules,
        rng_seed: rand::random(),
        targets: &hit_targets,
    });

    // Write back modified stats (stacking etc.)
    if output.hit_any {
        if let Some(mut stats) = stats {
            stats.0 = output.attacker_stats;
        }
    }

    // Fire events per hit
    let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let mut any_crit = false;

    for hit in &output.hits {
        // Look up the target entity and position
        let Some(&(target_entity, target_pos)) = target_list
            .iter()
            .find(|(e, _)| e.to_bits() == hit.target_id)
        else {
            continue;
        };

        // Shared knockback displacement — same function server uses
        let to_target = target_pos - attacker_pos;
        let radial_2d = Vec2::new(to_target.x, to_target.z);
        let fwd_2d = Vec2::new(forward_flat.x, forward_flat.z);
        let radial_dir = radial_2d.normalize_or(fwd_2d);
        let force = wasm_fantasia_shared::combat::knockback_displacement(
            radial_dir,
            fwd_2d,
            hit.knockback,
            hit.push,
            hit.launch,
        );

        commands.trigger(DamageDealt {
            source: attacker_entity,
            target: target_entity,
            damage: hit.damage,
            force,
            is_crit: hit.is_crit,
            feedback: hit.feedback.clone(),
        });

        if hit.is_crit {
            any_crit = true;
        }
    }

    attack_state.is_crit = any_crit;
}

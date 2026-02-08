use super::*;
use crate::player::control::InputBuffer;
use crate::rules::{OnPreHitRules, Stat, Stats};
use bevy_enhanced_input::prelude::Fire;
use bevy_tnua::builtins::TnuaBuiltinDash;
use bevy_tnua::prelude::*;
use wasm_fantasia_shared::combat::{AttackInput, cone_hit_check, defaults, resolve_attack};

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
    mut query: Query<(&mut AttackState, &TnuaController), With<PlayerCombatant>>,
) {
    let Ok((mut attack_state, controller)) = query.get_mut(on.context) else {
        return;
    };

    // Block attacks during dash/slide - buffer for after dash
    if controller.action_name() == Some(TnuaBuiltinDash::NAME) {
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
    mut query: Query<(&mut AttackState, &TnuaController), With<PlayerCombatant>>,
) {
    if buffer.attack.is_none() {
        return;
    }

    let Ok((mut attack_state, controller)) = query.single_mut() else {
        return;
    };

    if controller.action_name() == Some(TnuaBuiltinDash::NAME) {
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

        if state.attacking {
            state.attack_time += time.delta_secs() * speed_mult;

            if !state.hit_triggered && state.attack_time >= state.hit_time {
                commands.trigger(AttackHit { attacker: entity });
                state.hit_triggered = true;
            }

            if state.attack_time >= state.attack_duration {
                state.attacking = false;
                state.attack_time = 0.0;
                state.is_crit = false;
            }
        }
    }
}

/// Observer: triggered when attack hit time is reached.
/// Executes OnPreHitRules to compute damage, crit, force, and feedback values.
fn on_attack_hit(
    trigger: On<AttackHit>,
    mut attackers: Query<
        (
            &mut AttackState,
            &Transform,
            Option<&OnPreHitRules>,
            Option<&mut Stats>,
        ),
        With<PlayerCombatant>,
    >,
    targets: Query<(Entity, &Transform), (With<Health>, With<Enemy>)>,
    mut commands: Commands,
) {
    let attacker_entity = trigger.event().attacker;
    let Ok((mut attack_state, transform, pre_hit_rules, stats)) =
        attackers.get_mut(attacker_entity)
    else {
        return;
    };

    // Build attacker stats for shared resolution
    let attacker_stats = stats.as_ref().map(|s| s.0.clone()).unwrap_or_default();

    let base_range = attacker_stats.get(&Stat::AttackRange);
    let base_range = if base_range > 0.0 {
        base_range
    } else {
        defaults::ATTACK_RANGE
    };
    let base_arc = attacker_stats.get(&Stat::AttackArc);
    let base_arc = if base_arc > 0.0 {
        base_arc
    } else {
        defaults::ATTACK_ARC
    };

    let rng_roll: f32 = rand::random();
    let result = resolve_attack(&AttackInput {
        attacker_stats,
        pre_hit_rules: pre_hit_rules.map(|r| r.0.clone()).unwrap_or_default(),
        rng_roll,
    });

    let is_crit = result.is_crit;
    let damage = result.damage;
    let force_radial = result.knockback;
    let force_forward = result.push;
    let force_vertical = result.launch;
    let feedback = result.feedback;

    attack_state.is_crit = is_crit;

    let attacker_pos = transform.translation;
    let forward = transform.forward().as_vec3();
    let half_arc_cos = (base_arc / 2.0_f32).to_radians().cos();
    let forward_xz = Vec2::new(forward.x, forward.z).normalize_or_zero();
    let origin_xz = Vec2::new(attacker_pos.x, attacker_pos.z);

    // Crits get 30% more range
    let range = if is_crit {
        base_range * 1.3
    } else {
        base_range
    };

    for (target_entity, target_tf) in targets.iter() {
        let target_xz = Vec2::new(target_tf.translation.x, target_tf.translation.z);

        if !cone_hit_check(origin_xz, forward_xz, target_xz, range, half_arc_cos) {
            continue;
        }

        let to_target = target_tf.translation - attacker_pos;
        let to_target_flat = Vec3::new(to_target.x, 0.0, to_target.z).normalize_or_zero();
        let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let radial_dir = to_target_flat.normalize_or(forward_flat);
        let force =
            radial_dir * force_radial + forward_flat * force_forward + Vec3::Y * force_vertical;

        commands.trigger(DamageEvent {
            source: attacker_entity,
            target: target_entity,
            damage,
            force,
            is_crit,
            feedback: feedback.clone(),
        });
    }
}

use super::*;
use crate::player::control::InputBuffer;
use crate::rule_presets::feedback;
use crate::rules::{
    execute_effects, execute_rules, Action, ActionVar, OnPreHitRules, RuleOutput, Stat, Stats,
};
use bevy_enhanced_input::prelude::Fire;
use bevy_tnua::builtins::TnuaBuiltinDash;
use bevy_tnua::prelude::*;

/// Visual constants for attack effects
pub const VFX_RANGE: f32 = 2.0;
pub const VFX_ARC_DEGREES: f32 = 120.0;

/// Base values for combat - used to initialize Action context.
/// Rules can modify these. All combatants should have Stats.
mod base {
    // Attack parameters
    pub const DAMAGE: f32 = 25.0;
    pub const KNOCKBACK: f32 = 3.0;
    pub const RANGE: f32 = 3.6;
    pub const ARC: f32 = 150.0;
}

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
        // Buffer the attack for when current attack finishes
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

    // Still dashing, keep buffer
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
        // AttackSpeed: 1.0 = normal, >1 = faster, <1 = slower
        // Default to 1.0 if stat not set (0.0 means "not initialized", not "zero speed")
        let speed_mult = stats
            .map(|s| {
                let speed = s.get(&Stat::AttackSpeed);
                if speed == 0.0 { 1.0 } else { speed }
            })
            .unwrap_or(1.0)
            .max(0.1); // Prevent negative speed

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

    // Read base values from Stats, fall back to base constants
    let get_stat = |stat: &Stat, default: f32| -> f32 {
        stats
            .as_ref()
            .map(|s| s.get(stat))
            .filter(|&v| v > 0.0)
            .unwrap_or(default)
    };

    let base_damage = get_stat(&Stat::AttackDamage, base::DAMAGE);
    let base_knockback = get_stat(&Stat::Knockback, base::KNOCKBACK);
    let base_range = get_stat(&Stat::AttackRange, base::RANGE);
    let base_arc = get_stat(&Stat::AttackArc, base::ARC);

    // Create action context with base combat values
    let mut action = Action::new()
        .with(ActionVar::Damage, base_damage)
        .with(ActionVar::Knockback, base_knockback)
        .with(ActionVar::Push, 0.0)
        .with(ActionVar::Launch, 0.0);

    // Apply standard feedback preset (sets HitStopDuration, ShakeIntensity, etc.)
    let mut dummy_stats = Stats::new();
    let _ = execute_effects(&feedback::standard(), &mut dummy_stats, &mut action);

    // Execute OnPreHitRules (may modify damage, trigger crit, etc.)
    let rule_output = if let (Some(rules), Some(mut stats)) = (pre_hit_rules, stats) {
        execute_rules(&rules.0, &mut stats, &mut action)
    } else {
        RuleOutput::new()
    };

    // Check if crit was triggered by rules
    let is_crit = rule_output.is_crit();

    // Read computed values from action context
    let damage = action.get(&ActionVar::Damage);
    let force_radial = action.get(&ActionVar::Knockback);
    let force_forward = action.get(&ActionVar::Push);
    let force_vertical = action.get(&ActionVar::Launch);

    // Build feedback from action context
    let rumble_intensity = action.get(&ActionVar::RumbleIntensity);
    let feedback = HitFeedback {
        hit_stop_duration: action.get(&ActionVar::HitStopDuration),
        shake_intensity: action.get(&ActionVar::ShakeIntensity),
        flash_duration: action.get(&ActionVar::FlashDuration),
        // Map single intensity to both motors (strong gets full, weak gets 60%)
        rumble_strong: rumble_intensity,
        rumble_weak: rumble_intensity * 0.6,
        rumble_duration: action.get(&ActionVar::RumbleDuration),
    };

    attack_state.is_crit = is_crit;

    let attacker_pos = transform.translation;
    let forward = transform.forward().as_vec3();
    let half_arc_cos = (base_arc / 2.0_f32).to_radians().cos();

    // Crits get 30% more range
    let range = if is_crit {
        base_range * 1.3
    } else {
        base_range
    };

    for (target_entity, target_tf) in targets.iter() {
        let to_target = target_tf.translation - attacker_pos;
        let distance = to_target.length();

        if distance > range {
            continue;
        }

        let to_target_flat = Vec3::new(to_target.x, 0.0, to_target.z).normalize_or_zero();
        let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let dot = forward_flat.dot(to_target_flat);

        if dot < half_arc_cos {
            continue;
        }

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

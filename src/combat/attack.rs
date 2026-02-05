use super::*;
use crate::rules::{execute_rules, OnPreHitRules, RuleVars, Var};
use bevy_enhanced_input::prelude::Start;
use bevy_tnua::builtins::TnuaBuiltinDash;
use bevy_tnua::prelude::*;

/// Attack configuration - designed for hitting hordes
/// VFX defines the visual, hitbox is larger so attacks feel responsive
pub const VFX_RANGE: f32 = 2.0; // Visual range of the attack effect
pub const VFX_ARC_DEGREES: f32 = 120.0; // Visual arc in degrees
pub const ATTACK_RANGE: f32 = 3.6; // Hitbox extends beyond visuals (+20%)
pub const ATTACK_ARC: f32 = 150.0; // Hitbox arc wider than visuals
pub const ATTACK_DAMAGE: f32 = 25.0;
pub const ATTACK_KNOCKBACK: f32 = 3.0;

/// Speed bonus per stack (12% faster per stack)
const SPEED_PER_STACK: f32 = 0.12;

pub fn plugin(app: &mut App) {
    app.add_observer(handle_attack)
        .add_observer(on_attack_connect)
        .add_systems(Update, tick_attack_state.run_if(in_state(Screen::Gameplay)));
}

fn handle_attack(
    on: On<Start<Attack>>,
    mut query: Query<(&mut AttackState, &TnuaController), With<PlayerCombatant>>,
) {
    let Ok((mut attack_state, controller)) = query.get_mut(on.context) else {
        info!("handle_attack: no player found");
        return;
    };

    // Block attacks during dash/slide
    if controller.action_name() == Some(TnuaBuiltinDash::NAME) {
        return;
    }

    if attack_state.can_attack() {
        info!("Starting attack #{}", attack_state.attack_count + 1);
        attack_state.start_attack(false);
    } else {
        info!(
            "Can't attack: cooldown_finished={}, attacking={}",
            attack_state.cooldown.is_finished(),
            attack_state.attacking
        );
    }
}

/// Tick attack state timers and trigger hits based on time (not animation events).
fn tick_attack_state(
    time: Res<Time>,
    mut query: Query<(Entity, &mut AttackState, Option<&RuleVars>)>,
    mut commands: Commands,
) {
    for (entity, mut state, rule_vars) in query.iter_mut() {
        let stacks = rule_vars.map(|v| v.get(Var::Stacks)).unwrap_or(0.0);
        let speed_mult = 1.0 + (stacks * SPEED_PER_STACK);

        let scaled_delta = time.delta().mul_f32(speed_mult);
        state.cooldown.tick(scaled_delta);

        if state.attacking {
            state.attack_time += time.delta_secs() * speed_mult;

            if !state.hit_triggered && state.attack_time >= state.hit_time {
                commands.trigger(AttackConnect { attacker: entity });
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
/// Executes OnPreHitRules to determine crit/damage, then applies to all enemies in range.
fn on_attack_connect(
    trigger: On<AttackConnect>,
    mut attackers: Query<
        (
            &mut AttackState,
            &Transform,
            Option<&OnPreHitRules>,
            Option<&mut RuleVars>,
        ),
        With<PlayerCombatant>,
    >,
    targets: Query<(Entity, &Transform), (With<Health>, With<Enemy>)>,
    mut commands: Commands,
) {
    let attacker_entity = trigger.event().attacker;
    let Ok((mut attack_state, transform, pre_hit_rules, rule_vars)) =
        attackers.get_mut(attacker_entity)
    else {
        return;
    };

    let half_arc_cos = (ATTACK_ARC / 2.0_f32).to_radians().cos();

    // Execute OnPreHitRules to determine damage, crit, and force
    let (damage, force_radial, force_forward, force_vertical, is_crit) =
        if let (Some(rules), Some(mut vars)) = (pre_hit_rules, rule_vars) {
            // Set base values (also resets IsCrit from previous hit)
            vars.set(Var::HitDamage, ATTACK_DAMAGE);
            vars.set(Var::HitForceRadial, ATTACK_KNOCKBACK);
            vars.set(Var::HitForceForward, 0.0);
            vars.set(Var::HitForceVertical, 0.0);
            vars.set(Var::IsCrit, 0.0);

            // Execute rules (may modify damage, force, set crit)
            execute_rules(&rules.0, &mut vars);

            // Read results
            let dmg = vars.get(Var::HitDamage);
            let radial = vars.get(Var::HitForceRadial);
            let forward = vars.get(Var::HitForceForward);
            let vertical = vars.get(Var::HitForceVertical);
            let crit = vars.get(Var::IsCrit) > 0.5;

            attack_state.is_crit = crit;

            (dmg, radial, forward, vertical, crit)
        } else {
            (ATTACK_DAMAGE, ATTACK_KNOCKBACK, 0.0, 0.0, false)
        };

    let attacker_pos = transform.translation;
    let forward = transform.forward().as_vec3();
    let range = if is_crit {
        ATTACK_RANGE * 1.3
    } else {
        ATTACK_RANGE
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
        });
    }
}

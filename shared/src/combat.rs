//! Shared combat logic — constants, hit detection, timing, feedback.

/// Default combat stats — single source of truth for client and server.
pub mod defaults {
    pub const HEALTH: f32 = 100.0;
    pub const ATTACK_DAMAGE: f32 = 25.0;
    pub const CRIT_CHANCE: f32 = 0.20;
    pub const CRIT_MULTIPLIER: f32 = 2.5;
    pub const ATTACK_RANGE: f32 = 3.6;
    pub const ATTACK_ARC: f32 = 150.0;
    pub const KNOCKBACK: f32 = 1.0;
    pub const ATTACK_SPEED: f32 = 1.0;
    pub const STACK_DECAY: f32 = 2.5;
    pub const ATTACK_COOLDOWN_SECS: f32 = 0.42;
    pub const ENEMY_HEALTH: f32 = 500.0;
}

/// Attack timing constants (at 1.0x speed)
pub mod attack_timing {
    /// Base duration for punch animations (jab/cross)
    pub const PUNCH_DURATION: f32 = 0.42;
    /// Base duration for crit/hook animation (slower wind-up, bigger impact)
    pub const HOOK_DURATION: f32 = 0.55;
}

/// When the hit happens in each attack animation (fraction of duration)
pub mod hit_timing {
    pub const PUNCH_HIT_FRACTION: f32 = 0.55;
    pub const HOOK_HIT_FRACTION: f32 = 0.50;
}

/// Per-action feedback configuration, computed by rules.
#[derive(Debug, Clone, Default)]
pub struct HitFeedback {
    pub hit_stop_duration: f32,
    pub shake_intensity: f32,
    pub flash_duration: f32,
    pub rumble_strong: f32,
    pub rumble_weak: f32,
    pub rumble_duration: f32,
}

impl HitFeedback {
    /// Standard melee hit feedback values.
    pub fn standard(is_crit: bool) -> Self {
        Self {
            hit_stop_duration: 0.04,
            shake_intensity: 0.25,
            flash_duration: if is_crit { 0.15 } else { 0.08 },
            rumble_strong: 0.35,
            rumble_weak: 0.21,
            rumble_duration: 60.0,
        }
    }
}

/// 2D cone check on XZ plane. Returns true if target is within range and arc.
pub fn cone_hit_check(
    origin: glam::Vec2,
    forward: glam::Vec2,
    target: glam::Vec2,
    range: f32,
    half_arc_cos: f32,
) -> bool {
    let delta = target - origin;
    let dist = delta.length();

    if dist > range {
        return false;
    }

    if dist > 0.01 {
        let dir = delta / dist;
        let dot = forward.dot(dir);
        if dot < half_arc_cos {
            return false;
        }
    }

    true
}

/// Decay stacks to 0 if enough time has passed since last hit.
pub fn decay_stacks(stacks: f32, elapsed_secs: f64, decay_threshold: f32) -> f32 {
    if elapsed_secs > decay_threshold as f64 && stacks > 0.0 {
        0.0
    } else {
        stacks
    }
}

/// Check if enough time has passed since last attack (respecting attack speed).
pub fn can_attack(last_attack_micros: i64, now_micros: i64, attack_speed: f32) -> bool {
    let cooldown_micros =
        (defaults::ATTACK_COOLDOWN_SECS as f64 * 1_000_000.0 / attack_speed as f64) as i64;
    now_micros - last_attack_micros >= cooldown_micros
}

// ============================================================================
// UNIFIED COMBAT RESOLUTION
// ============================================================================

use crate::presets::feedback;
use crate::presets::EntityRules;
use crate::rules::{
    Action, ActionVar, Rule, RuleOutput, Stats, execute_effects, execute_rules,
    execute_rules_with_roll,
};

/// Input to the shared attack resolver.
pub struct AttackInput {
    pub attacker_stats: Stats,
    pub pre_hit_rules: Vec<Rule>,
    pub rng_roll: f32,
}

/// Output from the shared attack resolver.
pub struct AttackOutput {
    pub damage: f32,
    pub is_crit: bool,
    pub knockback: f32,
    pub push: f32,
    pub launch: f32,
    pub feedback: HitFeedback,
    pub rule_output: RuleOutput,
}

/// Unified attack resolution. Both client and server call this with identical inputs
/// to produce identical outputs. Deterministic when `rng_roll` is computed from
/// shared RNG with the same seeds.
pub fn resolve_attack(input: &AttackInput) -> AttackOutput {
    let stats = &input.attacker_stats;

    let base_damage = {
        let v = stats.get(&crate::rules::Stat::AttackDamage);
        if v > 0.0 { v } else { defaults::ATTACK_DAMAGE }
    };
    let base_knockback = {
        let v = stats.get(&crate::rules::Stat::Knockback);
        if v > 0.0 { v } else { defaults::KNOCKBACK }
    };

    // Build action context with base combat values
    let mut action = Action::new()
        .with(ActionVar::Damage, base_damage)
        .with(ActionVar::Knockback, base_knockback)
        .with(ActionVar::Push, 0.0)
        .with(ActionVar::Launch, 0.0);

    // Apply standard feedback preset
    let mut dummy_stats = Stats::new();
    let _ = execute_effects(&feedback::standard(), &mut dummy_stats, &mut action);

    // Execute pre-hit rules with deterministic roll
    let mut eval_stats = input.attacker_stats.clone();
    let rule_output = execute_rules_with_roll(
        &input.pre_hit_rules,
        &mut eval_stats,
        &mut action,
        input.rng_roll,
    );

    let is_crit = rule_output.is_crit();
    let damage = action.get(&ActionVar::Damage);
    let knockback = action.get(&ActionVar::Knockback);
    let push = action.get(&ActionVar::Push);
    let launch = action.get(&ActionVar::Launch);

    let rumble_intensity = action.get(&ActionVar::RumbleIntensity);
    let feedback = HitFeedback {
        hit_stop_duration: action.get(&ActionVar::HitStopDuration),
        shake_intensity: action.get(&ActionVar::ShakeIntensity),
        flash_duration: action.get(&ActionVar::FlashDuration),
        rumble_strong: rumble_intensity,
        rumble_weak: rumble_intensity * 0.6,
        rumble_duration: action.get(&ActionVar::RumbleDuration),
    };

    AttackOutput {
        damage,
        is_crit,
        knockback,
        push,
        launch,
        feedback,
        rule_output,
    }
}

// ============================================================================
// FULL COMBAT RESOLUTION (multi-target)
// ============================================================================

/// Crits extend hit range by 30%.
pub const CRIT_RANGE_BONUS: f32 = 1.3;

/// A potential target for the attack.
pub struct HitTarget {
    pub id: u64,
    pub pos: glam::Vec2,
    pub health: f32,
}

/// Result for a single target that was hit.
pub struct HitResult {
    pub target_id: u64,
    pub damage: f32,
    pub is_crit: bool,
    pub knockback: f32,
    pub push: f32,
    pub launch: f32,
    pub new_health: f32,
    pub died: bool,
    pub feedback: HitFeedback,
}

/// Everything needed to resolve a full attack against multiple targets.
pub struct CombatInput<'a> {
    pub origin: glam::Vec2,
    pub forward: glam::Vec2,
    pub base_range: f32,
    pub half_arc_cos: f32,
    pub attacker_stats: &'a Stats,
    pub rules: &'a EntityRules,
    pub rng_seed: u64,
    pub targets: &'a [HitTarget],
}

/// Results from [`resolve_combat`].
pub struct CombatOutput {
    /// Per-target hit results (only targets that passed the cone check).
    pub hits: Vec<HitResult>,
    /// Attacker stats after on_hit/on_crit_hit/on_kill rule execution.
    pub attacker_stats: Stats,
    pub hit_any: bool,
}

/// Full attack resolution: per-target RNG, damage calc, cone check,
/// health calc, death check, and on_hit/on_crit_hit/on_kill rule dispatch.
///
/// Callers build [`CombatInput`] from their storage (ECS or DB), call this,
/// then apply [`CombatOutput`] back. Zero game logic lives in the callers.
pub fn resolve_combat(input: &CombatInput) -> CombatOutput {
    let mut hits = Vec::new();
    let mut rule_stats = input.attacker_stats.clone();
    let mut hit_any = false;

    for target in input.targets {
        // Per-target deterministic RNG
        let rng_roll = crate::rng::deterministic_random_u64(input.rng_seed as i64, target.id);

        let result = resolve_attack(&AttackInput {
            attacker_stats: input.attacker_stats.clone(),
            pre_hit_rules: input.rules.pre_hit.clone(),
            rng_roll,
        });

        // Crits get bonus range
        let range = if result.is_crit {
            input.base_range * CRIT_RANGE_BONUS
        } else {
            input.base_range
        };

        if !cone_hit_check(input.origin, input.forward, target.pos, range, input.half_arc_cos) {
            continue;
        }

        let new_health = (target.health - result.damage).max(0.0);
        let died = new_health <= 0.0;

        hits.push(HitResult {
            target_id: target.id,
            damage: result.damage,
            is_crit: result.is_crit,
            knockback: result.knockback,
            push: result.push,
            launch: result.launch,
            new_health,
            died,
            feedback: result.feedback,
        });

        // On-hit rules accumulate on the attacker's stats (e.g. stacking)
        let mut action = Action::new();
        execute_rules(&input.rules.on_hit, &mut rule_stats, &mut action);

        if result.is_crit {
            let mut action = Action::new();
            execute_rules(&input.rules.on_crit_hit, &mut rule_stats, &mut action);
        }

        if died {
            let mut action = Action::new();
            execute_rules(&input.rules.on_kill, &mut rule_stats, &mut action);
        }

        hit_any = true;
    }

    CombatOutput {
        hits,
        attacker_stats: rule_stats,
        hit_any,
    }
}

//! Rule trigger components and observers

use super::*;
use crate::combat::{AttackState, DamageDealt};
use serde::{Deserialize, Serialize};

// ============================================================================
// DERIVED EVENTS
// ============================================================================

/// Sub-feedback: a critical hit occurred. See [`HitLanded`].
#[derive(Event, Clone, Debug)]
pub struct CritHit {
    pub source: Entity,
    pub target: Entity,
}

/// Sub-feedback: a critical kill occurred. See [`Died`].
#[derive(Event, Clone, Debug)]
pub struct CritKill {
    pub killer: Entity,
    pub victim: Entity,
}

// ============================================================================
// LEVEL 6: TRIGGER COMPONENTS
// ============================================================================

/// Rules that execute before a hit resolves (attacker).
/// Use to compute damage, determine crit, modify force.
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct OnPreHitRules(pub Vec<Rule>);

/// Rules that execute after a hit lands (attacker).
/// Use for on-hit effects like lifesteal, stack gain.
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct OnHitRules(pub Vec<Rule>);

/// Rules that execute after a critical hit (attacker).
/// Fires automatically when IsCrit is set (see module-level boolean convention).
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct OnCritHitRules(pub Vec<Rule>);

/// Rules that execute after killing an enemy (killer).
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct OnKillRules(pub Vec<Rule>);

/// Rules that execute after a critical kill (killer).
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct OnCritKillRules(pub Vec<Rule>);

/// Rules that execute when taking damage (defender).
/// Use for damage reduction, shields, armor.
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct OnTakeDamageRules(pub Vec<Rule>);

/// Rules that execute every frame.
/// DeltaTime is available in Action context.
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct OnTickRules(pub Vec<Rule>);

// ============================================================================
// OBSERVERS
// ============================================================================

/// On-hit/on-crit-hit/on-kill rule execution is now handled by
/// `resolve_combat()` in the shared crate. These observers only
/// dispatch sub-feedback events for VFX that care about crits.

fn on_take_damage_observer(
    trigger: On<DamageDealt>,
    mut query: Query<(&OnTakeDamageRules, &mut Stats)>,
) {
    let event = trigger.event();
    let mut action = Action::new();

    if let Ok((rules, mut stats)) = query.get_mut(event.target) {
        let _ = execute_rules(&rules.0, &mut stats.0, &mut action);
    }
}

fn tick_rules_system(time: Res<Time>, mut query: Query<(&OnTickRules, &mut Stats)>) {
    let delta = time.delta_secs();

    for (rules, mut stats) in query.iter_mut() {
        let mut action = Action::new().with(ActionVar::DeltaTime, delta);
        let _ = execute_rules(&rules.0, &mut stats.0, &mut action);
    }
}

/// Syncs AttackState component fields to Stats so rules can react to attack phases.
fn sync_attack_state_to_stats(mut query: Query<(&AttackState, &mut Stats)>) {
    for (attack_state, mut stats) in query.iter_mut() {
        stats.set(
            Stat::IsAttacking,
            if attack_state.is_attacking() { 1.0 } else { 0.0 },
        );

        stats.set(Stat::AttackProgress, attack_state.progress());
        stats.set(Stat::ComboCount, attack_state.attack_count as f32);

        stats.set(
            Stat::InWindup,
            if attack_state.in_windup() { 1.0 } else { 0.0 },
        );
        stats.set(
            Stat::InRecovery,
            if attack_state.in_recovery() { 1.0 } else { 0.0 },
        );
    }
}

// ============================================================================
// PLUGIN
// ============================================================================

pub fn plugin(app: &mut App) {
    use super::RulePresetPlugin;
    use crate::models::Screen;

    app.add_plugins(RulePresetPlugin)
        .add_observer(on_take_damage_observer)
        .add_systems(
            Update,
            (sync_attack_state_to_stats, tick_rules_system)
                .chain()
                .run_if(in_state(Screen::Gameplay)),
        );
}

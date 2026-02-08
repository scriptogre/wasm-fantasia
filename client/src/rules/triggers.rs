//! Rule trigger components and observers

use super::*;
use crate::combat::{AttackState, DamageEvent, DeathEvent, HitEvent};
use serde::{Deserialize, Serialize};

// ============================================================================
// DERIVED EVENTS
// ============================================================================

#[derive(Event, Clone, Debug)]
pub struct CritHitEvent {
    pub source: Entity,
    pub target: Entity,
}

#[derive(Event, Clone, Debug)]
pub struct CritKillEvent {
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

fn on_hit_observer(
    trigger: On<HitEvent>,
    mut query: Query<(&OnHitRules, &mut Stats)>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let mut action = Action::new();

    if let Ok((rules, mut stats)) = query.get_mut(event.source) {
        let output = execute_rules(&rules.0, &mut stats.0, &mut action);

        // Emit CritHitEvent if crit was triggered
        if output.is_crit() {
            commands.trigger(CritHitEvent {
                source: event.source,
                target: event.target,
            });
        }
    }
}

fn on_crit_hit_observer(
    trigger: On<CritHitEvent>,
    mut query: Query<(&OnCritHitRules, &mut Stats)>,
) {
    let event = trigger.event();
    let mut action = Action::new();

    if let Ok((rules, mut stats)) = query.get_mut(event.source) {
        let _ = execute_rules(&rules.0, &mut stats.0, &mut action);
    }
}

fn on_kill_observer(
    trigger: On<DeathEvent>,
    mut query: Query<(&OnKillRules, &mut Stats)>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let mut action = Action::new();

    if let Ok((rules, mut stats)) = query.get_mut(event.killer) {
        let output = execute_rules(&rules.0, &mut stats.0, &mut action);

        if output.is_crit() {
            commands.trigger(CritKillEvent {
                killer: event.killer,
                victim: event.entity,
            });
        }
    }
}

fn on_crit_kill_observer(
    trigger: On<CritKillEvent>,
    mut query: Query<(&OnCritKillRules, &mut Stats)>,
) {
    let event = trigger.event();
    let mut action = Action::new();

    if let Ok((rules, mut stats)) = query.get_mut(event.killer) {
        let _ = execute_rules(&rules.0, &mut stats.0, &mut action);
    }
}

fn on_take_damage_observer(
    trigger: On<DamageEvent>,
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
        // Boolean: 1.0 = true, 0.0 = false
        stats.set(
            Stat::IsAttacking,
            if attack_state.attacking { 1.0 } else { 0.0 },
        );

        stats.set(Stat::AttackProgress, attack_state.progress());
        stats.set(Stat::ComboCount, attack_state.attack_count as f32);

        // Windup = attacking but hit hasn't triggered yet
        let in_windup = attack_state.attacking && !attack_state.hit_triggered;
        stats.set(Stat::InWindup, if in_windup { 1.0 } else { 0.0 });

        // Recovery = attacking and hit has already triggered
        let in_recovery = attack_state.attacking && attack_state.hit_triggered;
        stats.set(Stat::InRecovery, if in_recovery { 1.0 } else { 0.0 });
    }
}

// ============================================================================
// PLUGIN
// ============================================================================

pub fn plugin(app: &mut App) {
    use super::RulePresetPlugin;
    use crate::models::Screen;

    app.add_plugins(RulePresetPlugin)
        .add_observer(on_hit_observer)
        .add_observer(on_crit_hit_observer)
        .add_observer(on_kill_observer)
        .add_observer(on_crit_kill_observer)
        .add_observer(on_take_damage_observer)
        .add_systems(
            Update,
            (sync_attack_state_to_stats, tick_rules_system)
                .chain()
                .run_if(in_state(Screen::Gameplay)),
        );
}

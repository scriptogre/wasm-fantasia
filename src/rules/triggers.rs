//! Rule trigger components and observers

use super::*;
use crate::combat::{DamageEvent, DeathEvent, HitEvent};

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
// TRIGGER COMPONENTS
// ============================================================================

#[derive(Component, Default, Clone, Debug)]
pub struct OnPreHitRules(pub Vec<Rule>);

#[derive(Component, Default, Clone, Debug)]
pub struct OnHitRules(pub Vec<Rule>);

#[derive(Component, Default, Clone, Debug)]
pub struct OnCritHitRules(pub Vec<Rule>);

#[derive(Component, Default, Clone, Debug)]
pub struct OnKillRules(pub Vec<Rule>);

#[derive(Component, Default, Clone, Debug)]
pub struct OnCritKillRules(pub Vec<Rule>);

#[derive(Component, Default, Clone, Debug)]
pub struct OnTakeDamageRules(pub Vec<Rule>);

#[derive(Component, Default, Clone, Debug)]
pub struct OnTickRules(pub Vec<Rule>);

// ============================================================================
// OBSERVERS
// ============================================================================

fn on_hit_observer(
    trigger: On<HitEvent>,
    mut query: Query<(&OnHitRules, &mut RuleVars)>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if let Ok((rules, mut vars)) = query.get_mut(event.source) {
        execute_rules(&rules.0, &mut vars);

        // Emit CritHitEvent if IsCrit flag is set
        if vars.get(Var::IsCrit) > 0.5 {
            commands.trigger(CritHitEvent {
                source: event.source,
                target: event.target,
            });
        }
    }
}

fn on_crit_hit_observer(
    trigger: On<CritHitEvent>,
    mut query: Query<(&OnCritHitRules, &mut RuleVars)>,
) {
    let event = trigger.event();
    if let Ok((rules, mut vars)) = query.get_mut(event.source) {
        execute_rules(&rules.0, &mut vars);
    }
}

fn on_kill_observer(
    trigger: On<DeathEvent>,
    mut query: Query<(&OnKillRules, &mut RuleVars)>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if let Ok((rules, mut vars)) = query.get_mut(event.killer) {
        execute_rules(&rules.0, &mut vars);

        if vars.get(Var::IsCrit) > 0.5 {
            commands.trigger(CritKillEvent {
                killer: event.killer,
                victim: event.entity,
            });
        }
    }
}

fn on_crit_kill_observer(
    trigger: On<CritKillEvent>,
    mut query: Query<(&OnCritKillRules, &mut RuleVars)>,
) {
    let event = trigger.event();
    if let Ok((rules, mut vars)) = query.get_mut(event.killer) {
        execute_rules(&rules.0, &mut vars);
    }
}

fn on_take_damage_observer(
    trigger: On<DamageEvent>,
    mut query: Query<(&OnTakeDamageRules, &mut RuleVars)>,
) {
    let event = trigger.event();
    if let Ok((rules, mut vars)) = query.get_mut(event.target) {
        execute_rules(&rules.0, &mut vars);
    }
}

fn tick_rules_system(time: Res<Time>, mut query: Query<(&OnTickRules, &mut RuleVars)>) {
    let delta = time.delta_secs();
    for (rules, mut vars) in query.iter_mut() {
        vars.set(Var::DeltaTime, delta);
        execute_rules(&rules.0, &mut vars);
    }
}

// ============================================================================
// TIMER RULES (periodic)
// ============================================================================

#[derive(Clone, Debug)]
pub struct TimerRule {
    pub rule: Rule,
    pub timer: Timer,
}

impl TimerRule {
    pub fn new(interval_secs: f32, rule: Rule) -> Self {
        Self {
            rule,
            timer: Timer::from_seconds(interval_secs, TimerMode::Repeating),
        }
    }
}

#[derive(Component, Default, Clone, Debug)]
pub struct OnTimerRules(pub Vec<TimerRule>);

fn timer_rules_system(time: Res<Time>, mut query: Query<(&mut OnTimerRules, &mut RuleVars)>) {
    for (mut timer_rules, mut vars) in query.iter_mut() {
        for tr in timer_rules.0.iter_mut() {
            tr.timer.tick(time.delta());
            if tr.timer.just_finished() && check_conditions(&tr.rule.conditions, &vars) {
                execute_effects(&tr.rule.effects, &mut vars);
            }
        }
    }
}

// ============================================================================
// PLUGIN
// ============================================================================

pub fn plugin(app: &mut App) {
    use crate::models::Screen;

    app.add_observer(on_hit_observer)
        .add_observer(on_crit_hit_observer)
        .add_observer(on_kill_observer)
        .add_observer(on_crit_kill_observer)
        .add_observer(on_take_damage_observer)
        .add_systems(
            Update,
            (tick_rules_system, timer_rules_system).run_if(in_state(Screen::Gameplay)),
        );
}

//! RON-loadable rule presets

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    OnCritHitRules, OnHitRules, OnKillRules, OnPreHitRules, OnTakeDamageRules, OnTickRules, Rule,
    Stats,
};
use crate::asset_loading::RonAssetPlugin;

// ============================================================================
// ASSET DEFINITION
// ============================================================================

/// A rule preset loaded from RON files.
///
/// Example RON:
/// ```ron
/// (
///     stats: { CritChance: 0.2, CritMultiplier: 2.5 },
///     on_pre_hit: [(
///         conditions: [Chance(Stat(CritChance))],
///         effects: [
///             SetAction(var: IsCrit, value: Value(1.0)),
///             SetAction(var: Damage, value: Multiply(Action(Damage), Stat(CritMultiplier))),
///         ],
///     )],
/// )
/// ```
#[derive(Asset, Clone, Debug, Default, Serialize, Deserialize, TypePath)]
pub struct RulePreset {
    #[serde(default)]
    pub stats: Stats,
    #[serde(default)]
    pub on_pre_hit: Vec<Rule>,
    #[serde(default)]
    pub on_hit: Vec<Rule>,
    #[serde(default)]
    pub on_crit_hit: Vec<Rule>,
    #[serde(default)]
    pub on_kill: Vec<Rule>,
    #[serde(default)]
    pub on_take_damage: Vec<Rule>,
    #[serde(default)]
    pub on_tick: Vec<Rule>,
}

impl RulePreset {
    /// Insert this preset's components onto an entity.
    pub fn insert_into(self, commands: &mut EntityCommands) {
        commands.insert(self.stats);
        if !self.on_pre_hit.is_empty() {
            commands.insert(OnPreHitRules(self.on_pre_hit));
        }
        if !self.on_hit.is_empty() {
            commands.insert(OnHitRules(self.on_hit));
        }
        if !self.on_crit_hit.is_empty() {
            commands.insert(OnCritHitRules(self.on_crit_hit));
        }
        if !self.on_kill.is_empty() {
            commands.insert(OnKillRules(self.on_kill));
        }
        if !self.on_take_damage.is_empty() {
            commands.insert(OnTakeDamageRules(self.on_take_damage));
        }
        if !self.on_tick.is_empty() {
            commands.insert(OnTickRules(self.on_tick));
        }
    }
}

// ============================================================================
// EXTENSION TRAIT
// ============================================================================

/// Extension trait for spawning rule presets on entities.
pub trait RulePresetCommands {
    /// Insert rules from a preset.
    fn with_preset(&mut self, preset: &RulePreset) -> &mut Self;
}

impl RulePresetCommands for EntityCommands<'_> {
    fn with_preset(&mut self, preset: &RulePreset) -> &mut Self {
        self.insert(preset.stats.clone());

        if !preset.on_pre_hit.is_empty() {
            self.insert(OnPreHitRules(preset.on_pre_hit.clone()));
        }
        if !preset.on_hit.is_empty() {
            self.insert(OnHitRules(preset.on_hit.clone()));
        }
        if !preset.on_crit_hit.is_empty() {
            self.insert(OnCritHitRules(preset.on_crit_hit.clone()));
        }
        if !preset.on_kill.is_empty() {
            self.insert(OnKillRules(preset.on_kill.clone()));
        }
        if !preset.on_take_damage.is_empty() {
            self.insert(OnTakeDamageRules(preset.on_take_damage.clone()));
        }
        if !preset.on_tick.is_empty() {
            self.insert(OnTickRules(preset.on_tick.clone()));
        }

        self
    }
}

// ============================================================================
// PLUGIN
// ============================================================================

pub struct RulePresetPlugin;

impl Plugin for RulePresetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RonAssetPlugin::<RulePreset>::default());
    }
}

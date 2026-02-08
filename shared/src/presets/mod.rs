pub mod crit;
pub mod feedback;
pub mod stacking;

use crate::rules::Rule;

/// Complete set of rules for an entity, grouped by trigger point.
/// Both client and server consume this — the client wraps each field
/// in a Bevy component, the server runs them directly.
pub struct EntityRules {
    pub pre_hit: Vec<Rule>,
    pub on_hit: Vec<Rule>,
    pub on_crit_hit: Vec<Rule>,
    pub on_tick: Vec<Rule>,
    pub on_kill: Vec<Rule>,
    pub on_take_damage: Vec<Rule>,
}

/// Default player rules. Single source of truth for client and server.
pub fn default_player_rules() -> EntityRules {
    let stacking = stacking::stacking_rules(stacking::StackingConfig::default());
    EntityRules {
        pre_hit: crit::crit_rules(),
        on_hit: stacking.on_hit,
        on_crit_hit: stacking.on_crit_hit,
        on_tick: stacking.on_tick,
        on_kill: vec![],
        on_take_damage: vec![],
    }
}

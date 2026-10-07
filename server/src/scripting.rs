use std::sync::Arc;

use game_core::runtime::{Effect, Intent, ScriptRegistry};

use game_core::runtime::registry::DEFAULT_BEHAVIORS;

thread_local! {
    static SCRIPTS: Arc<ScriptRegistry> = Arc::new(ScriptRegistry::build_builtin());
}

/// Execute a melee attack ability via the Rune scripting engine.
/// Returns empty results on script error instead of panicking the module.
pub fn run_melee_attack(
    source: game_core::runtime::Combatant,
    targets: Vec<game_core::runtime::Combatant>,
    rng_roll: f32,
) -> (Vec<Intent>, Vec<Effect>) {
    SCRIPTS.with(|reg| {
        let Some(engine) = reg.get("melee_attack") else {
            spacetimedb::log::warn!("melee_attack script not registered");
            return (vec![], vec![]);
        };
        let behaviors = DEFAULT_BEHAVIORS.iter().map(|id| (*id).into()).collect();
        match engine.call_ability_with_behaviors(
            "on_ability_start",
            source,
            targets,
            rng_roll,
            reg.clone(),
            behaviors,
        ) {
            Ok(result) => result,
            Err(e) => {
                spacetimedb::log::warn!("melee_attack script error: {e}");
                (vec![], vec![])
            }
        }
    })
}

/// Execute a ground pound ability via the Rune scripting engine.
/// Returns empty results on script error instead of panicking the module.
pub fn run_ground_pound(
    source: game_core::runtime::Combatant,
    targets: Vec<game_core::runtime::Combatant>,
    rng_roll: f32,
) -> (Vec<Intent>, Vec<Effect>) {
    SCRIPTS.with(|reg| {
        let Some(engine) = reg.get("ground_pound") else {
            spacetimedb::log::warn!("ground_pound script not registered");
            return (vec![], vec![]);
        };
        let behaviors = DEFAULT_BEHAVIORS.iter().map(|id| (*id).into()).collect();
        match engine.call_ability_with_behaviors(
            "on_ability_start",
            source,
            targets,
            rng_roll,
            reg.clone(),
            behaviors,
        ) {
            Ok(result) => result,
            Err(e) => {
                spacetimedb::log::warn!("ground_pound script error: {e}");
                (vec![], vec![])
            }
        }
    })
}

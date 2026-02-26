use std::sync::Arc;

use game_core::scripting::ScriptRegistry;

thread_local! {
    static SCRIPTS: Arc<ScriptRegistry> = {
        let mut reg = ScriptRegistry::new();
        reg.register(
            "crit".into(),
            include_str!("../../client/assets/scripts/behaviors/crit.rune"),
        )
        .expect("crit script should compile");
        reg.register(
            "stacking".into(),
            include_str!("../../client/assets/scripts/behaviors/stacking.rune"),
        )
        .expect("stacking script should compile");
        reg.register(
            "melee_attack".into(),
            include_str!("../../client/assets/scripts/abilities/melee_attack.rune"),
        )
        .expect("melee_attack script should compile");
        reg.register(
            "ground_pound".into(),
            include_str!("../../client/assets/scripts/abilities/ground_pound.rune"),
        )
        .expect("ground_pound script should compile");
        Arc::new(reg)
    };
}

/// Execute a melee attack ability via the Rune scripting engine.
pub fn run_melee_attack(
    source: game_core::scripting::Combatant,
    targets: Vec<game_core::scripting::Combatant>,
    rng_roll: f32,
) -> Vec<game_core::scripting::Command> {
    SCRIPTS.with(|reg| {
        let engine = reg.get("melee_attack").expect("melee_attack script must be registered");
        let behaviors = vec!["crit".into(), "stacking".into()];
        engine
            .call_ability_with_behaviors(
                "on_ability_start",
                source,
                targets,
                rng_roll,
                reg.clone(),
                behaviors,
            )
            .expect("melee_attack script execution failed")
    })
}

/// Execute a ground pound ability via the Rune scripting engine.
pub fn run_ground_pound(
    source: game_core::scripting::Combatant,
    targets: Vec<game_core::scripting::Combatant>,
    rng_roll: f32,
) -> Vec<game_core::scripting::Command> {
    SCRIPTS.with(|reg| {
        let engine = reg.get("ground_pound").expect("ground_pound script must be registered");
        let behaviors = vec!["crit".into(), "stacking".into()];
        engine
            .call_ability_with_behaviors(
                "on_ability_start",
                source,
                targets,
                rng_roll,
                reg.clone(),
                behaviors,
            )
            .expect("ground_pound script execution failed")
    })
}

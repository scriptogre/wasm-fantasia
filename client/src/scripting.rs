//! Bevy plugin for Rune script loading and execution.
//!
//! Wraps the `game_core::scripting` engine as Bevy resources and components,
//! compiling `.rune` scripts at startup via `include_str!()`.

use std::sync::Arc;

use bevy::prelude::*;
use game_core::scripting::registry::ScriptRegistry;

/// Bevy resource wrapping an `Arc<ScriptRegistry>` that holds all compiled scripts.
#[derive(Resource)]
pub struct ScriptRegistryRes(pub Arc<ScriptRegistry>);

/// Behavior scripts attached to an entity (e.g. `["crit", "stacking"]`).
/// These are chained via `fire_hook` during ability execution.
#[derive(Component, Clone, Debug, Default)]
pub struct EntityBehaviors(pub Vec<String>);

/// Which ability script this entity uses (e.g. `"melee_attack"`).
#[derive(Component, Clone, Debug)]
pub struct ActiveAbility(pub String);

fn build_registry() -> ScriptRegistry {
    let mut registry = ScriptRegistry::new();

    // Behavior scripts
    registry
        .register(
            "crit".to_string(),
            include_str!("../assets/scripts/behaviors/crit.rune"),
        )
        .expect("crit.rune should compile");

    registry
        .register(
            "stacking".to_string(),
            include_str!("../assets/scripts/behaviors/stacking.rune"),
        )
        .expect("stacking.rune should compile");

    registry
        .register(
            "feedback".to_string(),
            include_str!("../assets/scripts/behaviors/feedback.rune"),
        )
        .expect("feedback.rune should compile");

    // Ability scripts
    registry
        .register(
            "melee_attack".to_string(),
            include_str!("../assets/scripts/abilities/melee_attack.rune"),
        )
        .expect("melee_attack.rune should compile");

    registry
        .register(
            "ground_pound".to_string(),
            include_str!("../assets/scripts/abilities/ground_pound.rune"),
        )
        .expect("ground_pound.rune should compile");

    // Enemy AI scripts
    registry
        .register(
            "zombie_ai".to_string(),
            include_str!("../assets/scripts/enemies/zombie_ai.rune"),
        )
        .expect("zombie_ai.rune should compile");

    registry
}

pub fn plugin(app: &mut App) {
    app.insert_resource(ScriptRegistryRes(Arc::new(build_registry())));
}

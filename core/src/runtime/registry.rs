use std::collections::HashMap;

use super::ScriptEngine;

/// A collection of named behavior scripts that can be looked up by `fire_hook`.
pub struct ScriptRegistry {
    scripts: HashMap<String, ScriptEngine>,
}

impl ScriptRegistry {
    pub fn new() -> Self {
        Self {
            scripts: HashMap::new(),
        }
    }

    /// Compile and register a behavior script under the given name.
    pub fn register(&mut self, name: String, source: &str) -> Result<(), rune::support::Error> {
        let engine = ScriptEngine::new(source)?;
        self.scripts.insert(name, engine);
        Ok(())
    }

    /// Look up a compiled script by name.
    pub fn get(&self, name: &str) -> Option<&ScriptEngine> {
        self.scripts.get(name)
    }
}

impl Default for ScriptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_loadout_has_executable_hooks() {
        let registry = ScriptRegistry::build_builtin();
        for id in DEFAULT_ABILITIES {
            assert!(registry.get(id).unwrap().has_function("on_ability_start"));
        }
        for id in DEFAULT_BEHAVIORS {
            let engine = registry.get(id).unwrap();
            assert!(engine.has_function("on_pre_hit") || engine.has_function("on_hit"));
        }
    }

    #[test]
    fn register_and_get() {
        let mut registry = ScriptRegistry::new();
        registry
            .register("test".to_string(), "pub fn on_hit() { 42 }")
            .expect("should compile");
        assert!(registry.get("test").is_some());
        assert!(registry.get("missing").is_none());
    }
}

/// Presentation metadata for the same scripts registered by both game hosts.
pub struct ScriptDefinition {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub description: &'static str,
    pub source: &'static str,
}

pub const DEFAULT_ABILITIES: &[&str] = &["melee_attack", "ground_pound"];
pub const DEFAULT_BEHAVIORS: &[&str] = &["crit", "stacking"];

pub const BUILTIN_SCRIPTS: &[ScriptDefinition] = &[
    ScriptDefinition {
        id: "melee_attack",
        title: "Melee",
        icon: "melee",
        description: "Strike enemies in front of you. Hold Attack to keep striking.",
        source: include_str!("../../runes/abilities/melee_attack.rune"),
    },
    ScriptDefinition {
        id: "ground_pound",
        title: "Slam",
        icon: "slam",
        description: "Attack while airborne to slam the ground and hit nearby enemies.",
        source: include_str!("../../runes/abilities/ground_pound.rune"),
    },
    ScriptDefinition {
        id: "crit",
        title: "Critical hit",
        icon: "crit",
        description: "A chance to multiply a strike's damage and knockback before it lands.",
        source: include_str!("../../runes/behaviors/crit.rune"),
    },
    ScriptDefinition {
        id: "stacking",
        title: "Fury",
        icon: "fury",
        description: "Landed hits build a short-lived attack speed bonus. Keep hitting to sustain it.",
        source: include_str!("../../runes/behaviors/stacking.rune"),
    },
    ScriptDefinition {
        id: "feedback",
        title: "Hit feedback",
        icon: "impact",
        description: "Sound and visual feedback for a landed hit.",
        source: include_str!("../../runes/behaviors/feedback.rune"),
    },
];

impl ScriptRegistry {
    pub fn build_builtin() -> Self {
        let mut registry = Self::new();
        for script in BUILTIN_SCRIPTS {
            registry
                .register(script.id.into(), script.source)
                .expect("built-in script compiles");
        }
        registry
    }
}

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
    fn register_and_get() {
        let mut registry = ScriptRegistry::new();
        registry
            .register(
                "test".to_string(),
                "pub fn on_hit() { 42 }",
            )
            .expect("should compile");
        assert!(registry.get("test").is_some());
        assert!(registry.get("missing").is_none());
    }
}

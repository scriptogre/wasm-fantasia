pub mod commands;
pub mod game_module;
pub mod types;

use std::sync::Arc;

use rune::runtime::{Unit, Vm};
use rune::{Context, Diagnostics, Source, Sources};

pub use commands::{Command, CommandBuffer};
pub use game_module::{set_rng_roll, take_commands};
pub use types::{Combatant, Hit};

use game_module::build_game_module;

/// A wrapper around the Rune scripting engine with the game module installed.
///
/// Compiles Rune source code and executes named functions within it.
/// Provides `call_hit_hook` for combat behavior scripts.
pub struct ScriptEngine {
    runtime: Arc<rune::runtime::RuntimeContext>,
    unit: Arc<Unit>,
}

impl ScriptEngine {
    /// Compile a Rune script from a source string with the game module installed.
    ///
    /// Returns an error if the script contains syntax or compilation errors.
    pub fn new(source: &str) -> Result<Self, rune::support::Error> {
        let mut context = Context::with_default_modules()?;
        context.install(build_game_module()?)?;

        let runtime = Arc::new(context.runtime()?);

        let mut sources = Sources::new();
        let _ = sources.insert(Source::memory(source)?);

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        let unit = Arc::new(result?);

        Ok(Self { runtime, unit })
    }

    /// Call a named function with no arguments and return the result as an `i64`.
    pub fn call_i64(&mut self, name: &str) -> Result<i64, rune::support::Error> {
        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        let output = vm.call([name], ())?;
        let value: i64 = rune::from_value(output)?;
        Ok(value)
    }

    /// Check whether the compiled script contains a function with the given name.
    pub fn has_function(&self, name: &str) -> bool {
        let vm = Vm::new(self.runtime.clone(), self.unit.clone());
        vm.lookup_function([name]).is_ok()
    }

    /// Call a hit hook function: `fn hook(source, target, hit) -> Hit`.
    ///
    /// Sets the RNG roll, calls the function, and returns the (possibly modified)
    /// `Hit` along with any commands emitted during execution.
    pub fn call_hit_hook(
        &mut self,
        function: &str,
        source: Combatant,
        target: Combatant,
        hit: Hit,
        rng_roll: f32,
    ) -> Result<(Hit, Vec<Command>), rune::support::Error> {
        // Clear any stale commands and set the RNG roll.
        let _ = take_commands();
        set_rng_roll(rng_roll);

        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        let output = vm.call([function], (source, target, hit))?;
        let modified_hit: Hit = rune::from_value(output)?;
        let cmds = take_commands();

        Ok((modified_hit, cmds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_and_call_trivial_script() {
        let mut engine =
            ScriptEngine::new("pub fn hello() { 42 }").expect("script should compile");
        let result = engine.call_i64("hello").expect("call should succeed");
        assert_eq!(result, 42);
    }

    #[test]
    fn compile_script_with_game_module() {
        let script = r#"
            use game::*;

            pub fn on_hit(source, target, hit) {
                hit
            }
        "#;
        ScriptEngine::new(script).expect("script using game module should compile");
    }

    #[test]
    fn has_function_returns_true_for_existing() {
        let engine =
            ScriptEngine::new("pub fn my_hook() { 1 }").expect("script should compile");
        assert!(engine.has_function("my_hook"));
    }

    #[test]
    fn has_function_returns_false_for_missing() {
        let engine =
            ScriptEngine::new("pub fn my_hook() { 1 }").expect("script should compile");
        assert!(!engine.has_function("nonexistent"));
    }

    #[test]
    fn script_emits_commands() {
        let script = r#"
            use game::*;

            pub fn on_hit(source, target, hit) {
                damage(target, 50.0);
                vfx("sparks", target);
                hit
            }
        "#;
        let mut engine = ScriptEngine::new(script).expect("script should compile");

        let source = Combatant {
            id: 1,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            dir_x: 1.0,
            dir_z: 0.0,
            health: 100.0,
            max_health: 100.0,
            attack_damage: 10.0,
            crit_chance: 0.0,
            crit_multiplier: 1.5,
            knockback_force: 5.0,
            attack_range: 2.0,
            attack_arc: 90.0,
            attack_speed: 1.0,
            fury_stacks: 0,
            attack_speed_bonus: 0.0,
            cooldown_ready: true,
            speed: 5.0,
        };
        let target = Combatant {
            id: 2,
            ..source.clone()
        };
        let hit = Hit {
            damage: 10.0,
            knockback: 1.0,
            is_crit: false,
        };

        let (_hit, cmds) = engine
            .call_hit_hook("on_hit", source, target, hit, 0.5)
            .expect("hook should succeed");

        assert_eq!(cmds.len(), 2, "expected 2 commands, got {cmds:?}");
        assert!(matches!(cmds[0], Command::DealDamage { target_id: 2, amount } if (amount - 50.0).abs() < f32::EPSILON));
        assert!(matches!(&cmds[1], Command::SpawnVfx { name, target_id: 2 } if name == "sparks"));
    }

    #[test]
    fn chance_uses_rng_roll() {
        let script = r#"
            use game::*;

            pub fn test_chance(source, target, hit) {
                // With roll=0.3, chance(0.5) should be true (0.3 < 0.5)
                if chance(0.5) {
                    Hit { damage: 999.0, knockback: 0.0, is_crit: true }
                } else {
                    hit
                }
            }
        "#;
        let mut engine = ScriptEngine::new(script).expect("script should compile");

        let combatant = Combatant {
            id: 1,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            dir_x: 1.0,
            dir_z: 0.0,
            health: 100.0,
            max_health: 100.0,
            attack_damage: 10.0,
            crit_chance: 0.0,
            crit_multiplier: 1.5,
            knockback_force: 5.0,
            attack_range: 2.0,
            attack_arc: 90.0,
            attack_speed: 1.0,
            fury_stacks: 0,
            attack_speed_bonus: 0.0,
            cooldown_ready: true,
            speed: 5.0,
        };
        let hit = Hit {
            damage: 10.0,
            knockback: 1.0,
            is_crit: false,
        };

        // roll=0.3 < probability=0.5 → chance returns true
        let (result_hit, _) = engine
            .call_hit_hook("test_chance", combatant.clone(), combatant.clone(), hit.clone(), 0.3)
            .expect("hook should succeed");
        assert!((result_hit.damage - 999.0).abs() < f32::EPSILON, "chance(0.5) with roll=0.3 should be true");
        assert!(result_hit.is_crit);

        // roll=0.7 >= probability=0.5 → chance returns false
        let (result_hit, _) = engine
            .call_hit_hook("test_chance", combatant.clone(), combatant, hit, 0.7)
            .expect("hook should succeed");
        assert!((result_hit.damage - 10.0).abs() < f32::EPSILON, "chance(0.5) with roll=0.7 should be false");
        assert!(!result_hit.is_crit);
    }

    #[test]
    fn script_can_modify_hit() {
        let script = r#"
            use game::*;

            pub fn amplify(source, target, hit) {
                Hit {
                    damage: hit.damage * 2.0,
                    knockback: hit.knockback,
                    is_crit: true,
                }
            }
        "#;
        let mut engine = ScriptEngine::new(script).expect("script should compile");

        let combatant = Combatant {
            id: 1,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            dir_x: 1.0,
            dir_z: 0.0,
            health: 100.0,
            max_health: 100.0,
            attack_damage: 10.0,
            crit_chance: 0.0,
            crit_multiplier: 1.5,
            knockback_force: 5.0,
            attack_range: 2.0,
            attack_arc: 90.0,
            attack_speed: 1.0,
            fury_stacks: 0,
            attack_speed_bonus: 0.0,
            cooldown_ready: true,
            speed: 5.0,
        };
        let hit = Hit {
            damage: 25.0,
            knockback: 3.0,
            is_crit: false,
        };

        let (result, _) = engine
            .call_hit_hook("amplify", combatant.clone(), combatant, hit, 0.0)
            .expect("hook should succeed");

        assert!((result.damage - 50.0).abs() < f32::EPSILON);
        assert!((result.knockback - 3.0).abs() < f32::EPSILON);
        assert!(result.is_crit);
    }
}

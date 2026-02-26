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

    // Helper to create a default combatant for tests.
    fn test_combatant(id: u64) -> Combatant {
        Combatant {
            id,
            pos_x: 0.0,
            pos_y: 1.0,
            pos_z: 0.0,
            dir_x: 1.0,
            dir_z: 0.0,
            health: 100.0,
            max_health: 100.0,
            attack_damage: 10.0,
            crit_chance: 0.2,
            crit_multiplier: 2.0,
            knockback_force: 5.0,
            attack_range: 2.0,
            attack_arc: 90.0,
            attack_speed: 1.0,
            fury_stacks: 0,
            attack_speed_bonus: 0.0,
            cooldown_ready: true,
            speed: 5.0,
        }
    }

    fn base_hit() -> Hit {
        Hit {
            damage: 20.0,
            knockback: 4.0,
            is_crit: false,
        }
    }

    // --- Crit behavior tests ---

    const CRIT_SCRIPT: &str = r#"
        use game::*;

        pub fn on_pre_hit(source, target, hit) {
            if chance(source.crit_chance) {
                Hit {
                    damage: hit.damage * source.crit_multiplier,
                    knockback: hit.knockback * source.crit_multiplier,
                    is_crit: true,
                }
            } else {
                hit
            }
        }
    "#;

    #[test]
    fn crit_script_crits_on_low_roll() {
        let mut engine = ScriptEngine::new(CRIT_SCRIPT).expect("crit script should compile");
        let source = test_combatant(1); // crit_chance=0.2, crit_multiplier=2.0
        let target = test_combatant(2);
        let hit = base_hit(); // damage=20, knockback=4

        // roll=0.1 < crit_chance=0.2 → crit
        let (result, _cmds) = engine
            .call_hit_hook("on_pre_hit", source, target, hit, 0.1)
            .expect("hook should succeed");

        assert!(result.is_crit, "should be a crit");
        assert!(
            (result.damage - 40.0).abs() < f32::EPSILON,
            "damage should be 20 * 2.0 = 40, got {}",
            result.damage
        );
        assert!(
            (result.knockback - 8.0).abs() < f32::EPSILON,
            "knockback should be 4 * 2.0 = 8, got {}",
            result.knockback
        );
    }

    #[test]
    fn crit_script_no_crit_on_high_roll() {
        let mut engine = ScriptEngine::new(CRIT_SCRIPT).expect("crit script should compile");
        let source = test_combatant(1);
        let target = test_combatant(2);
        let hit = base_hit();

        // roll=0.9 >= crit_chance=0.2 → no crit
        let (result, _cmds) = engine
            .call_hit_hook("on_pre_hit", source, target, hit, 0.9)
            .expect("hook should succeed");

        assert!(!result.is_crit, "should not be a crit");
        assert!(
            (result.damage - 20.0).abs() < f32::EPSILON,
            "damage should be unchanged at 20, got {}",
            result.damage
        );
        assert!(
            (result.knockback - 4.0).abs() < f32::EPSILON,
            "knockback should be unchanged at 4, got {}",
            result.knockback
        );
    }

    // --- Stacking behavior tests ---

    const STACKING_SCRIPT: &str = r#"
        use game::*;

        pub fn on_hit(source, target, hit) {
            let add = if hit.is_crit { 3 } else { 1 };
            let stacks_i = source.fury_stacks + add;
            let stacks = min(stacks_i as f64, 12.0);
            set_stat(source, "fury_stacks", stacks);
            set_stat(source, "attack_speed_bonus", stacks * 0.12);
            buff(source, "fury", 2.5);
            hit
        }
    "#;

    #[test]
    fn stacking_adds_one_on_normal_hit() {
        let mut engine = ScriptEngine::new(STACKING_SCRIPT).expect("stacking script should compile");
        let source = test_combatant(1); // fury_stacks=0
        let target = test_combatant(2);
        let hit = base_hit(); // is_crit=false

        let (_result, cmds) = engine
            .call_hit_hook("on_hit", source, target, hit, 0.5)
            .expect("hook should succeed");

        // Expect: SetStat(fury_stacks, 1.0), SetStat(attack_speed_bonus, 0.12), AddBuff(fury, 2.5)
        assert_eq!(cmds.len(), 3, "expected 3 commands, got {cmds:?}");
        assert!(
            matches!(&cmds[0], Command::SetStat { entity_id: 1, stat, value }
                if stat == "fury_stacks" && (*value - 1.0).abs() < f32::EPSILON),
            "first command should set fury_stacks to 1, got {:?}",
            cmds[0]
        );
        assert!(
            matches!(&cmds[1], Command::SetStat { entity_id: 1, stat, value }
                if stat == "attack_speed_bonus" && (*value - 0.12).abs() < 0.001),
            "second command should set attack_speed_bonus to 0.12, got {:?}",
            cmds[1]
        );
        assert!(
            matches!(&cmds[2], Command::AddBuff { target_id: 1, name, duration }
                if name == "fury" && (*duration - 2.5).abs() < f32::EPSILON),
            "third command should be AddBuff fury 2.5, got {:?}",
            cmds[2]
        );
    }

    #[test]
    fn stacking_adds_three_on_crit() {
        let mut engine = ScriptEngine::new(STACKING_SCRIPT).expect("stacking script should compile");
        let source = test_combatant(1); // fury_stacks=0
        let target = test_combatant(2);
        let hit = Hit {
            damage: 20.0,
            knockback: 4.0,
            is_crit: true,
        };

        let (_result, cmds) = engine
            .call_hit_hook("on_hit", source, target, hit, 0.5)
            .expect("hook should succeed");

        assert!(
            matches!(&cmds[0], Command::SetStat { entity_id: 1, stat, value }
                if stat == "fury_stacks" && (*value - 3.0).abs() < f32::EPSILON),
            "fury_stacks should be 3 on crit, got {:?}",
            cmds[0]
        );
    }

    // --- Feedback behavior tests ---

    const FEEDBACK_SCRIPT: &str = r#"
        use game::*;

        pub fn on_hit(source, target, hit) {
            let intensity = if hit.is_crit { 1.0 } else { 0.5 };
            vfx("hit_flash", target);
            sound("impact", target.pos_x, target.pos_y, target.pos_z);
            screen_shake(intensity);
            hit_stop(if hit.is_crit { 0.08 } else { 0.04 });
            hit
        }
    "#;

    #[test]
    fn feedback_emits_correct_commands() {
        let mut engine = ScriptEngine::new(FEEDBACK_SCRIPT).expect("feedback script should compile");
        let source = test_combatant(1);
        let target = test_combatant(2);
        let hit = base_hit(); // is_crit=false

        let (_result, cmds) = engine
            .call_hit_hook("on_hit", source, target, hit, 0.5)
            .expect("hook should succeed");

        assert_eq!(cmds.len(), 4, "expected 4 commands, got {cmds:?}");
        assert!(matches!(&cmds[0], Command::SpawnVfx { name, target_id: 2 } if name == "hit_flash"));
        assert!(matches!(&cmds[1], Command::PlaySound { name, .. } if name == "impact"));
        assert!(matches!(&cmds[2], Command::ScreenShake { .. }));
        assert!(matches!(&cmds[3], Command::HitStop { .. }));
    }

    #[test]
    fn feedback_stronger_on_crit() {
        let mut engine = ScriptEngine::new(FEEDBACK_SCRIPT).expect("feedback script should compile");
        let source = test_combatant(1);
        let target = test_combatant(2);

        // Normal hit
        let hit_normal = base_hit();
        let (_, cmds_normal) = engine
            .call_hit_hook("on_hit", source.clone(), target.clone(), hit_normal, 0.5)
            .expect("hook should succeed");
        assert!(
            matches!(&cmds_normal[2], Command::ScreenShake { intensity } if (*intensity - 0.5).abs() < f32::EPSILON),
            "normal hit screen_shake should be 0.5, got {:?}",
            cmds_normal[2]
        );
        assert!(
            matches!(&cmds_normal[3], Command::HitStop { duration } if (*duration - 0.04).abs() < f32::EPSILON),
            "normal hit hit_stop should be 0.04, got {:?}",
            cmds_normal[3]
        );

        // Crit hit
        let hit_crit = Hit {
            damage: 20.0,
            knockback: 4.0,
            is_crit: true,
        };
        let (_, cmds_crit) = engine
            .call_hit_hook("on_hit", source, target, hit_crit, 0.5)
            .expect("hook should succeed");
        assert!(
            matches!(&cmds_crit[2], Command::ScreenShake { intensity } if (*intensity - 1.0).abs() < f32::EPSILON),
            "crit screen_shake should be 1.0, got {:?}",
            cmds_crit[2]
        );
        assert!(
            matches!(&cmds_crit[3], Command::HitStop { duration } if (*duration - 0.08).abs() < f32::EPSILON),
            "crit hit_stop should be 0.08, got {:?}",
            cmds_crit[3]
        );
    }
}

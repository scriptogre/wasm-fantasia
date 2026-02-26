pub mod commands;
pub mod game_module;
pub mod registry;
pub mod types;

use std::rc::Rc;
use std::sync::Arc;

use rune::runtime::{Unit, Vm};
use rune::{Context, Diagnostics, Source, Sources};

pub use commands::{Command, CommandBuffer};
pub use game_module::{
    clear_script_registry, set_available_targets, set_entity_behaviors, set_rng_roll,
    set_script_registry, take_commands,
};
pub use registry::ScriptRegistry;
pub use types::{Combatant, Hit};

use game_module::build_game_module;

/// A wrapper around the Rune scripting engine with the game module installed.
///
/// Compiles Rune source code and executes named functions within it.
/// Provides `call_hit_hook` for combat behavior scripts.
pub struct ScriptEngine {
    pub(crate) runtime: Arc<rune::runtime::RuntimeContext>,
    pub(crate) unit: Arc<Unit>,
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
    pub fn call_i64(&self, name: &str) -> Result<i64, rune::support::Error> {
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

    /// Call an ability function: `fn on_ability_start(source)`.
    ///
    /// Sets the RNG roll and available targets, calls the function, and returns
    /// any commands emitted during execution.
    pub fn call_ability(
        &self,
        function: &str,
        source: Combatant,
        targets: Vec<Combatant>,
        rng_roll: f32,
    ) -> Result<Vec<Command>, rune::support::Error> {
        let _ = take_commands();
        set_rng_roll(rng_roll);
        game_module::set_available_targets(targets);

        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        vm.call([function], (source,))?;

        Ok(take_commands())
    }

    /// Call an ability function with behavior hook chaining support.
    ///
    /// Like `call_ability`, but also installs a `ScriptRegistry` and entity
    /// behaviors so that `fire_hook()` calls inside the script can chain through
    /// the behavior scripts.
    pub fn call_ability_with_behaviors(
        &self,
        function: &str,
        source: Combatant,
        targets: Vec<Combatant>,
        rng_roll: f32,
        registry: Rc<ScriptRegistry>,
        behaviors: Vec<String>,
    ) -> Result<Vec<Command>, rune::support::Error> {
        let _ = take_commands();
        set_rng_roll(rng_roll);
        set_available_targets(targets);
        set_entity_behaviors(behaviors);
        set_script_registry(registry);

        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        vm.call([function], (source,))?;

        clear_script_registry();
        Ok(take_commands())
    }

    /// Call a hit hook function: `fn hook(source, target, hit) -> Hit`.
    ///
    /// Sets the RNG roll, calls the function, and returns the (possibly modified)
    /// `Hit` along with any commands emitted during execution.
    pub fn call_hit_hook(
        &self,
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
    use std::rc::Rc;

    use super::*;

    #[test]
    fn compile_and_call_trivial_script() {
        let engine =
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
        let engine = ScriptEngine::new(script).expect("script should compile");

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
        let engine = ScriptEngine::new(script).expect("script should compile");

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
        let engine = ScriptEngine::new(script).expect("script should compile");

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
        let engine = ScriptEngine::new(CRIT_SCRIPT).expect("crit script should compile");
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
        let engine = ScriptEngine::new(CRIT_SCRIPT).expect("crit script should compile");
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
        let engine = ScriptEngine::new(STACKING_SCRIPT).expect("stacking script should compile");
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
        let engine = ScriptEngine::new(STACKING_SCRIPT).expect("stacking script should compile");
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
        let engine = ScriptEngine::new(FEEDBACK_SCRIPT).expect("feedback script should compile");
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
        let engine = ScriptEngine::new(FEEDBACK_SCRIPT).expect("feedback script should compile");
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

    // --- Ability script tests ---

    const MELEE_ATTACK_SCRIPT: &str = r#"
        use game::*;

        pub fn on_ability_start(source) {
            animate(source, "attack");
            sound("swoosh", source.pos_x, source.pos_y, source.pos_z);

            let targets = targets_in_cone(source, source.attack_range, source.attack_arc);

            for target in targets {
                damage(target, source.attack_damage);
                knockback(target, source.knockback_force);
            }
        }
    "#;

    #[test]
    fn melee_attack_hits_targets_in_cone() {
        let engine = ScriptEngine::new(MELEE_ATTACK_SCRIPT).expect("script should compile");

        // Source at origin facing +Z (dir_z = 1.0)
        let source = Combatant {
            id: 1,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            dir_x: 0.0,
            dir_z: 1.0,
            health: 100.0,
            max_health: 100.0,
            attack_damage: 10.0,
            crit_chance: 0.0,
            crit_multiplier: 1.5,
            knockback_force: 5.0,
            attack_range: 3.0,
            attack_arc: 90.0,
            attack_speed: 1.0,
            fury_stacks: 0,
            attack_speed_bonus: 0.0,
            cooldown_ready: true,
            speed: 5.0,
        };

        // Target in front (+Z direction, within range and arc)
        let target_front = Combatant {
            id: 2,
            pos_x: 0.0,
            pos_z: 2.0,
            ..source.clone()
        };

        // Target behind (-Z direction, outside arc)
        let target_behind = Combatant {
            id: 3,
            pos_x: 0.0,
            pos_z: -2.0,
            ..source.clone()
        };

        let targets = vec![target_front, target_behind];
        let cmds = engine
            .call_ability("on_ability_start", source, targets, 0.5)
            .expect("ability should succeed");

        // Expected commands: Animate, Sound, DealDamage(target 2), ApplyKnockback(target 2)
        assert_eq!(cmds.len(), 4, "expected 4 commands, got {cmds:?}");
        assert!(matches!(&cmds[0], Command::Animate { entity_id: 1, animation } if animation == "attack"));
        assert!(matches!(&cmds[1], Command::PlaySound { name, .. } if name == "swoosh"));
        assert!(matches!(cmds[2], Command::DealDamage { target_id: 2, amount } if (amount - 10.0).abs() < f32::EPSILON));
        assert!(matches!(cmds[3], Command::ApplyKnockback { target_id: 2, force } if (force - 5.0).abs() < f32::EPSILON));
    }

    const GROUND_POUND_SCRIPT: &str = r#"
        use game::*;

        pub fn on_ability_start(source) {
            animate(source, "ground_pound");
            sound("ground_pound", source.pos_x, source.pos_y, source.pos_z);
            vfx("ground_pound_shockwave", source);

            let targets = targets_in_radius(source.pos_x, source.pos_z, 6.0);
            let base_damage = source.attack_damage * 4.0;

            for target in targets {
                damage(target, base_damage);
                knockback(target, 20.0);
            }

            screen_shake(1.5);
        }
    "#;

    #[test]
    fn ground_pound_hits_targets_in_radius() {
        let engine = ScriptEngine::new(GROUND_POUND_SCRIPT).expect("script should compile");

        let source = Combatant {
            id: 1,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            dir_x: 0.0,
            dir_z: 1.0,
            health: 100.0,
            max_health: 100.0,
            attack_damage: 10.0,
            crit_chance: 0.0,
            crit_multiplier: 1.5,
            knockback_force: 5.0,
            attack_range: 3.0,
            attack_arc: 90.0,
            attack_speed: 1.0,
            fury_stacks: 0,
            attack_speed_bonus: 0.0,
            cooldown_ready: true,
            speed: 5.0,
        };

        // Close target at distance 3.0 (within 6.0 radius)
        let target_close = Combatant {
            id: 2,
            pos_x: 3.0,
            pos_z: 0.0,
            ..source.clone()
        };

        // Far target at distance 10.0 (outside 6.0 radius)
        let target_far = Combatant {
            id: 3,
            pos_x: 10.0,
            pos_z: 0.0,
            ..source.clone()
        };

        let targets = vec![target_close, target_far];
        let cmds = engine
            .call_ability("on_ability_start", source, targets, 0.5)
            .expect("ability should succeed");

        // Expected: Animate, Sound, SpawnVfx, DealDamage(target 2), ApplyKnockback(target 2), ScreenShake
        assert_eq!(cmds.len(), 6, "expected 6 commands, got {cmds:?}");
        assert!(matches!(&cmds[0], Command::Animate { entity_id: 1, animation } if animation == "ground_pound"));
        assert!(matches!(&cmds[1], Command::PlaySound { name, .. } if name == "ground_pound"));
        assert!(matches!(&cmds[2], Command::SpawnVfx { name, target_id: 1 } if name == "ground_pound_shockwave"));
        // Only the close target (id=2) should be hit: damage = 10.0 * 4.0 = 40.0
        assert!(matches!(cmds[3], Command::DealDamage { target_id: 2, amount } if (amount - 40.0).abs() < f32::EPSILON));
        assert!(matches!(cmds[4], Command::ApplyKnockback { target_id: 2, force } if (force - 20.0).abs() < f32::EPSILON));
        assert!(matches!(cmds[5], Command::ScreenShake { intensity } if (intensity - 1.5).abs() < f32::EPSILON));
    }

    // --- fire_hook tests ---

    const MELEE_ATTACK_WITH_HOOKS: &str = r#"
        use game::*;

        pub fn on_ability_start(source) {
            animate(source, "attack");
            sound("swoosh", source.pos_x, source.pos_y, source.pos_z);

            let targets = targets_in_cone(source, source.attack_range, source.attack_arc);

            for target in targets {
                let hit = Hit { damage: source.attack_damage, knockback: source.knockback_force, is_crit: false };

                hit = fire_hook("on_pre_hit", source, target, hit);

                damage(target, hit.damage);
                knockback(target, hit.knockback);

                hit = fire_hook("on_hit", source, target, hit);

                if hit.is_crit {
                    vfx("crit_particles", target);
                }
            }
        }
    "#;

    const GROUND_POUND_WITH_HOOKS: &str = r#"
        use game::*;

        pub fn on_ability_start(source) {
            animate(source, "ground_pound");
            sound("ground_pound", source.pos_x, source.pos_y, source.pos_z);
            vfx("ground_pound_shockwave", source);

            let targets = targets_in_radius(source.pos_x, source.pos_z, 6.0);
            let base_damage = source.attack_damage * 4.0;

            for target in targets {
                let hit = Hit { damage: base_damage, knockback: 20.0, is_crit: false };

                hit = fire_hook("on_pre_hit", source, target, hit);

                damage(target, hit.damage);
                knockback(target, hit.knockback);

                hit = fire_hook("on_hit", source, target, hit);
            }

            screen_shake(1.5);
        }
    "#;

    #[test]
    fn fire_hook_chains_behaviors() {
        // Set up a registry with crit and stacking behaviors
        let mut registry = ScriptRegistry::new();
        registry
            .register("crit".to_string(), CRIT_SCRIPT)
            .expect("crit should compile");
        registry
            .register("stacking".to_string(), STACKING_SCRIPT)
            .expect("stacking should compile");
        let registry = Rc::new(registry);

        let engine =
            ScriptEngine::new(MELEE_ATTACK_WITH_HOOKS).expect("melee attack with hooks should compile");

        // Source facing +Z, crit_chance=0.2, crit_multiplier=2.0
        let source = test_combatant(1);

        // One target in front, within cone
        let target = Combatant {
            id: 2,
            pos_x: 1.0,
            pos_z: 0.0,
            ..test_combatant(2)
        };

        let targets = vec![target];

        // rng_roll=0.1, which is < crit_chance=0.2, so crit triggers
        let cmds = engine
            .call_ability_with_behaviors(
                "on_ability_start",
                source,
                targets,
                0.1,
                registry,
                vec!["crit".to_string(), "stacking".to_string()],
            )
            .expect("ability should succeed");

        // Expected sequence:
        // 1. Animate(1, "attack")
        // 2. PlaySound("swoosh", ...)
        // -- fire_hook("on_pre_hit") runs crit.on_pre_hit (stacking has no on_pre_hit, skipped)
        //    crit triggers: damage 10*2=20, knockback 5*2=10, is_crit=true
        // 3. DealDamage(2, 20.0)  -- from ability script using modified hit
        // 4. ApplyKnockback(2, 10.0) -- from ability script using modified hit
        // -- fire_hook("on_hit") runs crit (no on_hit, skipped), stacking.on_hit
        //    stacking: is_crit=true → add=3, stacks=3, sets fury_stacks=3, attack_speed_bonus=0.36, buff fury 2.5
        // 5. SetStat(1, "fury_stacks", 3.0)
        // 6. SetStat(1, "attack_speed_bonus", 0.36)
        // 7. AddBuff(1, "fury", 2.5)
        // 8. SpawnVfx("crit_particles", 2) -- from ability script (is_crit check)

        assert_eq!(cmds.len(), 8, "expected 8 commands, got {cmds:?}");

        assert!(
            matches!(&cmds[0], Command::Animate { entity_id: 1, animation } if animation == "attack"),
            "cmd[0] should be Animate, got {:?}",
            cmds[0]
        );
        assert!(
            matches!(&cmds[1], Command::PlaySound { name, .. } if name == "swoosh"),
            "cmd[1] should be PlaySound swoosh, got {:?}",
            cmds[1]
        );
        // Crit doubled the damage: 10 * 2.0 = 20
        assert!(
            matches!(cmds[2], Command::DealDamage { target_id: 2, amount } if (amount - 20.0).abs() < f32::EPSILON),
            "cmd[2] should be DealDamage 20.0, got {:?}",
            cmds[2]
        );
        // Crit doubled the knockback: 5 * 2.0 = 10
        assert!(
            matches!(cmds[3], Command::ApplyKnockback { target_id: 2, force } if (force - 10.0).abs() < f32::EPSILON),
            "cmd[3] should be ApplyKnockback 10.0, got {:?}",
            cmds[3]
        );
        // Stacking on_hit commands (crit → 3 stacks)
        assert!(
            matches!(&cmds[4], Command::SetStat { entity_id: 1, stat, value }
                if stat == "fury_stacks" && (*value - 3.0).abs() < f32::EPSILON),
            "cmd[4] should be SetStat fury_stacks 3, got {:?}",
            cmds[4]
        );
        assert!(
            matches!(&cmds[5], Command::SetStat { entity_id: 1, stat, value }
                if stat == "attack_speed_bonus" && (*value - 0.36).abs() < 0.001),
            "cmd[5] should be SetStat attack_speed_bonus 0.36, got {:?}",
            cmds[5]
        );
        assert!(
            matches!(&cmds[6], Command::AddBuff { target_id: 1, name, duration }
                if name == "fury" && (*duration - 2.5).abs() < f32::EPSILON),
            "cmd[6] should be AddBuff fury 2.5, got {:?}",
            cmds[6]
        );
        // is_crit was true, so vfx("crit_particles", target) runs
        assert!(
            matches!(&cmds[7], Command::SpawnVfx { name, target_id: 2 } if name == "crit_particles"),
            "cmd[7] should be SpawnVfx crit_particles, got {:?}",
            cmds[7]
        );
    }

    #[test]
    fn fire_hook_skips_scripts_without_hook() {
        // Stacking has on_hit but NOT on_pre_hit.
        // Crit has on_pre_hit but NOT on_hit.
        // fire_hook("on_pre_hit") should only run crit, skip stacking.
        // fire_hook("on_hit") should only run stacking, skip crit.
        let mut registry = ScriptRegistry::new();
        registry
            .register("stacking".to_string(), STACKING_SCRIPT)
            .expect("stacking should compile");
        // Only stacking — no crit. fire_hook("on_pre_hit") should skip stacking (no on_pre_hit).
        let registry = Rc::new(registry);

        let engine =
            ScriptEngine::new(MELEE_ATTACK_WITH_HOOKS).expect("melee attack with hooks should compile");

        let source = test_combatant(1);
        let target = Combatant {
            id: 2,
            pos_x: 1.0,
            pos_z: 0.0,
            ..test_combatant(2)
        };

        let cmds = engine
            .call_ability_with_behaviors(
                "on_ability_start",
                source,
                vec![target],
                0.5, // high roll, no crit even if crit were present
                registry,
                vec!["stacking".to_string()],
            )
            .expect("ability should succeed");

        // fire_hook("on_pre_hit") — stacking has no on_pre_hit → hit unchanged
        // damage = source.attack_damage = 10.0 (no crit modification)
        // knockback = source.knockback_force = 5.0
        // fire_hook("on_hit") — stacking.on_hit runs: is_crit=false → add=1

        // Expected:
        // 1. Animate(1, "attack")
        // 2. PlaySound("swoosh")
        // 3. DealDamage(2, 10.0)  — unmodified
        // 4. ApplyKnockback(2, 5.0) — unmodified
        // 5. SetStat(1, fury_stacks, 1.0)
        // 6. SetStat(1, attack_speed_bonus, 0.12)
        // 7. AddBuff(1, fury, 2.5)
        // (no crit_particles since is_crit=false)

        assert_eq!(cmds.len(), 7, "expected 7 commands, got {cmds:?}");
        assert!(
            matches!(cmds[2], Command::DealDamage { target_id: 2, amount } if (amount - 10.0).abs() < f32::EPSILON),
            "damage should be unmodified at 10.0, got {:?}",
            cmds[2]
        );
        assert!(
            matches!(cmds[3], Command::ApplyKnockback { target_id: 2, force } if (force - 5.0).abs() < f32::EPSILON),
            "knockback should be unmodified at 5.0, got {:?}",
            cmds[3]
        );
        assert!(
            matches!(&cmds[4], Command::SetStat { entity_id: 1, stat, value }
                if stat == "fury_stacks" && (*value - 1.0).abs() < f32::EPSILON),
            "fury_stacks should be 1 (normal hit), got {:?}",
            cmds[4]
        );
    }

    #[test]
    fn fire_hook_with_no_behaviors_passes_hit_through() {
        // No behaviors attached — fire_hook should return hit unchanged
        let registry = Rc::new(ScriptRegistry::new());
        let engine =
            ScriptEngine::new(MELEE_ATTACK_WITH_HOOKS).expect("script should compile");

        let source = test_combatant(1);
        let target = Combatant {
            id: 2,
            pos_x: 1.0,
            pos_z: 0.0,
            ..test_combatant(2)
        };

        let cmds = engine
            .call_ability_with_behaviors(
                "on_ability_start",
                source,
                vec![target],
                0.5,
                registry,
                vec![],
            )
            .expect("ability should succeed");

        // No behaviors → no modifications, no stacking commands, no crit particles
        // Just: Animate, PlaySound, DealDamage(10.0), ApplyKnockback(5.0)
        assert_eq!(cmds.len(), 4, "expected 4 commands with no behaviors, got {cmds:?}");
        assert!(matches!(cmds[2], Command::DealDamage { target_id: 2, amount } if (amount - 10.0).abs() < f32::EPSILON));
        assert!(matches!(cmds[3], Command::ApplyKnockback { target_id: 2, force } if (force - 5.0).abs() < f32::EPSILON));
    }

    #[test]
    fn ground_pound_with_hooks_and_crit() {
        let mut registry = ScriptRegistry::new();
        registry
            .register("crit".to_string(), CRIT_SCRIPT)
            .expect("crit should compile");
        let registry = Rc::new(registry);

        let engine =
            ScriptEngine::new(GROUND_POUND_WITH_HOOKS).expect("ground pound with hooks should compile");

        let source = test_combatant(1);
        let target_close = Combatant {
            id: 2,
            pos_x: 3.0,
            pos_z: 0.0,
            ..test_combatant(2)
        };

        // rng_roll=0.1 < crit_chance=0.2 → crit
        let cmds = engine
            .call_ability_with_behaviors(
                "on_ability_start",
                source,
                vec![target_close],
                0.1,
                registry,
                vec!["crit".to_string()],
            )
            .expect("ability should succeed");

        // base_damage = 10 * 4 = 40, crit doubles it → 80
        // knockback = 20, crit doubles → 40
        // Expected: Animate, Sound, Vfx(shockwave), DealDamage(80), ApplyKnockback(40), ScreenShake
        assert_eq!(cmds.len(), 6, "expected 6 commands, got {cmds:?}");
        assert!(
            matches!(cmds[3], Command::DealDamage { target_id: 2, amount } if (amount - 80.0).abs() < f32::EPSILON),
            "ground pound crit damage should be 80, got {:?}",
            cmds[3]
        );
        assert!(
            matches!(cmds[4], Command::ApplyKnockback { target_id: 2, force } if (force - 40.0).abs() < f32::EPSILON),
            "ground pound crit knockback should be 40, got {:?}",
            cmds[4]
        );
    }
}

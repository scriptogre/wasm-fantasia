pub mod api;
pub mod registry;
pub mod types;

use std::sync::Arc;

use rune::runtime::{Unit, Vm};
use rune::{Context, Diagnostics, Source, Sources};

pub use api::{
    clear_logs, clear_script_registry, set_available_players, set_available_targets,
    set_entity_behaviors, set_rng_roll, set_script_registry, take_effects, take_intents, Effect,
    Intent,
};
pub use registry::ScriptRegistry;
pub use types::{Combatant, Hit};

use api::build_gameplay_module;

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
        context.install(build_gameplay_module()?)?;

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
    /// any intents and effects emitted during execution.
    pub fn call_ability(
        &self,
        function: &str,
        source: Combatant,
        targets: Vec<Combatant>,
        rng_roll: f32,
    ) -> Result<(Vec<Intent>, Vec<Effect>), rune::support::Error> {
        api::clear_logs();
        set_rng_roll(rng_roll);
        api::set_available_targets(targets);

        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        vm.call([function], (source,))?;

        Ok((take_intents(), take_effects()))
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
        registry: Arc<ScriptRegistry>,
        behaviors: Vec<String>,
    ) -> Result<(Vec<Intent>, Vec<Effect>), rune::support::Error> {
        api::clear_logs();
        set_rng_roll(rng_roll);
        set_available_targets(targets);
        set_entity_behaviors(behaviors);
        set_script_registry(registry);

        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        vm.call([function], (source,))?;

        api::clear_script_registry();
        Ok((take_intents(), take_effects()))
    }

    /// Call a tick function for AI scripts: `fn on_tick(entity, dt)`.
    ///
    /// Sets the RNG roll and available players, calls the function, and returns
    /// any intents and effects emitted during execution.
    pub fn call_tick(
        &self,
        function: &str,
        entity: Combatant,
        players: Vec<Combatant>,
        dt: f32,
        rng_roll: f32,
    ) -> Result<(Vec<Intent>, Vec<Effect>), rune::support::Error> {
        api::clear_logs();
        set_rng_roll(rng_roll);
        api::set_available_players(players);

        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        vm.call([function], (entity, dt))?;

        Ok((take_intents(), take_effects()))
    }

    /// Call a hit hook function: `fn hook(source, target, hit) -> Hit`.
    ///
    /// Sets the RNG roll, calls the function, and returns the (possibly modified)
    /// `Hit` along with any intents and effects emitted during execution.
    pub fn call_hit_hook(
        &self,
        function: &str,
        source: Combatant,
        target: Combatant,
        hit: Hit,
        rng_roll: f32,
    ) -> Result<(Hit, Vec<Intent>, Vec<Effect>), rune::support::Error> {
        api::clear_logs();
        set_rng_roll(rng_roll);

        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        let output = vm.call([function], (source, target, hit))?;
        let modified_hit: Hit = rune::from_value(output)?;

        Ok((modified_hit, take_intents(), take_effects()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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
            use gameplay::*;

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
    fn script_emits_intents_and_effects() {
        let script = r#"
            use gameplay::*;

            pub fn on_hit(source, target, hit) {
                apply_damage(target, 50.0);
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

        let (_hit, intents, effects) = engine
            .call_hit_hook("on_hit", source, target, hit, 0.5)
            .expect("hook should succeed");

        assert_eq!(intents.len(), 1, "expected 1 intent, got {intents:?}");
        assert!(matches!(intents[0], Intent::DamageDealt { target_id: 2, amount } if (amount - 50.0).abs() < f32::EPSILON));
        assert_eq!(effects.len(), 1, "expected 1 effect, got {effects:?}");
        assert!(matches!(&effects[0], Effect::Vfx { name, target_id: 2 } if name == "sparks"));
    }

    #[test]
    fn chance_uses_rng_roll() {
        let script = r#"
            use gameplay::*;

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
        let (result_hit, _, _) = engine
            .call_hit_hook("test_chance", combatant.clone(), combatant.clone(), hit.clone(), 0.3)
            .expect("hook should succeed");
        assert!((result_hit.damage - 999.0).abs() < f32::EPSILON, "chance(0.5) with roll=0.3 should be true");
        assert!(result_hit.is_crit);

        // roll=0.7 >= probability=0.5 → chance returns false
        let (result_hit, _, _) = engine
            .call_hit_hook("test_chance", combatant.clone(), combatant, hit, 0.7)
            .expect("hook should succeed");
        assert!((result_hit.damage - 10.0).abs() < f32::EPSILON, "chance(0.5) with roll=0.7 should be false");
        assert!(!result_hit.is_crit);
    }

    #[test]
    fn script_can_modify_hit() {
        let script = r#"
            use gameplay::*;

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

        let (result, _, _) = engine
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
        use gameplay::*;

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
        let (result, _intents, _effects) = engine
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
        let (result, _intents, _effects) = engine
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
        use gameplay::*;

        pub fn on_hit(source, target, hit) {
            let add = if hit.is_crit { 3 } else { 1 };
            let stacks_i = source.fury_stacks + add;
            let stacks = min(stacks_i as f64, 12.0);
            set_stat(source, "fury_stacks", stacks);
            set_stat(source, "attack_speed_bonus", stacks * 0.12);
            add_buff(source, "fury", 2.5);
            hit
        }
    "#;

    #[test]
    fn stacking_adds_one_on_normal_hit() {
        let engine = ScriptEngine::new(STACKING_SCRIPT).expect("stacking script should compile");
        let source = test_combatant(1); // fury_stacks=0
        let target = test_combatant(2);
        let hit = base_hit(); // is_crit=false

        let (_result, intents, _effects) = engine
            .call_hit_hook("on_hit", source, target, hit, 0.5)
            .expect("hook should succeed");

        // Expect: StatSet(fury_stacks, 1.0), StatSet(attack_speed_bonus, 0.12), BuffAdded(fury, 2.5)
        assert_eq!(intents.len(), 3, "expected 3 intents, got {intents:?}");
        assert!(
            matches!(&intents[0], Intent::StatSet { entity_id: 1, stat, value }
                if stat == "fury_stacks" && (*value - 1.0).abs() < f32::EPSILON),
            "first intent should set fury_stacks to 1, got {:?}",
            intents[0]
        );
        assert!(
            matches!(&intents[1], Intent::StatSet { entity_id: 1, stat, value }
                if stat == "attack_speed_bonus" && (*value - 0.12).abs() < 0.001),
            "second intent should set attack_speed_bonus to 0.12, got {:?}",
            intents[1]
        );
        assert!(
            matches!(&intents[2], Intent::BuffAdded { target_id: 1, name, duration }
                if name == "fury" && (*duration - 2.5).abs() < f32::EPSILON),
            "third intent should be BuffAdded fury 2.5, got {:?}",
            intents[2]
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

        let (_result, intents, _effects) = engine
            .call_hit_hook("on_hit", source, target, hit, 0.5)
            .expect("hook should succeed");

        assert!(
            matches!(&intents[0], Intent::StatSet { entity_id: 1, stat, value }
                if stat == "fury_stacks" && (*value - 3.0).abs() < f32::EPSILON),
            "fury_stacks should be 3 on crit, got {:?}",
            intents[0]
        );
    }

    // --- Feedback behavior tests ---

    const FEEDBACK_SCRIPT: &str = r#"
        use gameplay::*;

        pub fn on_hit(source, target, hit) {
            let intensity = if hit.is_crit { 1.0 } else { 0.5 };
            vfx("hit_flash", target);
            sound("impact", target);
            screen_shake(intensity);
            hit_stop(if hit.is_crit { 0.08 } else { 0.04 });
            hit
        }
    "#;

    #[test]
    fn feedback_emits_correct_effects() {
        let engine = ScriptEngine::new(FEEDBACK_SCRIPT).expect("feedback script should compile");
        let source = test_combatant(1);
        let target = test_combatant(2);
        let hit = base_hit(); // is_crit=false

        let (_result, intents, effects) = engine
            .call_hit_hook("on_hit", source, target, hit, 0.5)
            .expect("hook should succeed");

        assert!(intents.is_empty(), "feedback should emit no intents, got {intents:?}");
        assert_eq!(effects.len(), 4, "expected 4 effects, got {effects:?}");
        assert!(matches!(&effects[0], Effect::Vfx { name, target_id: 2 } if name == "hit_flash"));
        assert!(matches!(&effects[1], Effect::Sound { name, target_id: 2 } if name == "impact"));
        assert!(matches!(&effects[2], Effect::ScreenShake { .. }));
        assert!(matches!(&effects[3], Effect::HitStop { .. }));
    }

    #[test]
    fn feedback_stronger_on_crit() {
        let engine = ScriptEngine::new(FEEDBACK_SCRIPT).expect("feedback script should compile");
        let source = test_combatant(1);
        let target = test_combatant(2);

        // Normal hit
        let hit_normal = base_hit();
        let (_, _intents, effects_normal) = engine
            .call_hit_hook("on_hit", source.clone(), target.clone(), hit_normal, 0.5)
            .expect("hook should succeed");
        assert!(
            matches!(&effects_normal[2], Effect::ScreenShake { intensity } if (*intensity - 0.5).abs() < f32::EPSILON),
            "normal hit screen_shake should be 0.5, got {:?}",
            effects_normal[2]
        );
        assert!(
            matches!(&effects_normal[3], Effect::HitStop { duration } if (*duration - 0.04).abs() < f32::EPSILON),
            "normal hit hit_stop should be 0.04, got {:?}",
            effects_normal[3]
        );

        // Crit hit
        let hit_crit = Hit {
            damage: 20.0,
            knockback: 4.0,
            is_crit: true,
        };
        let (_, _intents, effects_crit) = engine
            .call_hit_hook("on_hit", source, target, hit_crit, 0.5)
            .expect("hook should succeed");
        assert!(
            matches!(&effects_crit[2], Effect::ScreenShake { intensity } if (*intensity - 1.0).abs() < f32::EPSILON),
            "crit screen_shake should be 1.0, got {:?}",
            effects_crit[2]
        );
        assert!(
            matches!(&effects_crit[3], Effect::HitStop { duration } if (*duration - 0.08).abs() < f32::EPSILON),
            "crit hit_stop should be 0.08, got {:?}",
            effects_crit[3]
        );
    }

    // --- Ability script tests ---

    const MELEE_ATTACK_SCRIPT: &str = r#"
        use gameplay::*;

        pub fn on_ability_start(source) {
            animate(source, "attack");
            sound("swoosh", source);

            let targets = targets_in_cone(source, source.attack_range, source.attack_arc);

            for target in targets {
                apply_damage(target, source.attack_damage);
                apply_knockback(target, source.knockback_force);
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
        let (intents, effects) = engine
            .call_ability("on_ability_start", source, targets, 0.5)
            .expect("ability should succeed");

        // Expected effects: Animate, Sound
        assert_eq!(effects.len(), 2, "expected 2 effects, got {effects:?}");
        assert!(matches!(&effects[0], Effect::Animate { entity_id: 1, animation } if animation == "attack"));
        assert!(matches!(&effects[1], Effect::Sound { name, target_id: 1 } if name == "swoosh"));

        // Expected intents: DamageDealt(target 2), KnockbackApplied(target 2)
        assert_eq!(intents.len(), 2, "expected 2 intents, got {intents:?}");
        assert!(matches!(intents[0], Intent::DamageDealt { target_id: 2, amount } if (amount - 10.0).abs() < f32::EPSILON));
        assert!(matches!(intents[1], Intent::KnockbackApplied { target_id: 2, force } if (force - 5.0).abs() < f32::EPSILON));
    }

    const GROUND_POUND_SCRIPT: &str = r#"
        use gameplay::*;

        pub fn on_ability_start(source) {
            animate(source, "ground_pound");
            sound("ground_pound", source);
            vfx("ground_pound_shockwave", source);

            let targets = targets_in_radius(source.pos_x, source.pos_z, 6.0);
            let base_damage = source.attack_damage * 4.0;

            for target in targets {
                apply_damage(target, base_damage);
                apply_knockback(target, 20.0);
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
        let (intents, effects) = engine
            .call_ability("on_ability_start", source, targets, 0.5)
            .expect("ability should succeed");

        // Expected effects: Animate, Sound, Vfx(shockwave), ScreenShake
        assert_eq!(effects.len(), 4, "expected 4 effects, got {effects:?}");
        assert!(matches!(&effects[0], Effect::Animate { entity_id: 1, animation } if animation == "ground_pound"));
        assert!(matches!(&effects[1], Effect::Sound { name, target_id: 1 } if name == "ground_pound"));
        assert!(matches!(&effects[2], Effect::Vfx { name, target_id: 1 } if name == "ground_pound_shockwave"));
        assert!(matches!(effects[3], Effect::ScreenShake { intensity } if (intensity - 1.5).abs() < f32::EPSILON));

        // Expected intents: DamageDealt(target 2, 40.0), KnockbackApplied(target 2, 20.0)
        // Only the close target (id=2) should be hit: damage = 10.0 * 4.0 = 40.0
        assert_eq!(intents.len(), 2, "expected 2 intents, got {intents:?}");
        assert!(matches!(intents[0], Intent::DamageDealt { target_id: 2, amount } if (amount - 40.0).abs() < f32::EPSILON));
        assert!(matches!(intents[1], Intent::KnockbackApplied { target_id: 2, force } if (force - 20.0).abs() < f32::EPSILON));
    }

    // --- fire_hook tests ---

    const MELEE_ATTACK_WITH_HOOKS: &str = r#"
        use gameplay::*;

        pub fn on_ability_start(source) {
            animate(source, "attack");
            sound("swoosh", source);

            let targets = targets_in_cone(source, source.attack_range, source.attack_arc);

            for target in targets {
                let hit = Hit { damage: source.attack_damage, knockback: source.knockback_force, is_crit: false };

                hit = fire_hook("on_pre_hit", source, target, hit);

                apply_damage(target, hit.damage);
                apply_knockback(target, hit.knockback);

                hit = fire_hook("on_hit", source, target, hit);

                if hit.is_crit {
                    vfx("crit_particles", target);
                }
            }
        }
    "#;

    const GROUND_POUND_WITH_HOOKS: &str = r#"
        use gameplay::*;

        pub fn on_ability_start(source) {
            animate(source, "ground_pound");
            sound("ground_pound", source);
            vfx("ground_pound_shockwave", source);

            let targets = targets_in_radius(source.pos_x, source.pos_z, 6.0);
            let base_damage = source.attack_damage * 4.0;

            for target in targets {
                let hit = Hit { damage: base_damage, knockback: 20.0, is_crit: false };

                hit = fire_hook("on_pre_hit", source, target, hit);

                apply_damage(target, hit.damage);
                apply_knockback(target, hit.knockback);

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
        let registry = Arc::new(registry);

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
        let (intents, effects) = engine
            .call_ability_with_behaviors(
                "on_ability_start",
                source,
                targets,
                0.1,
                registry,
                vec!["crit".to_string(), "stacking".to_string()],
            )
            .expect("ability should succeed");

        // Expected sequence of intents:
        // 1. DamageDealt(2, 20.0) -- crit doubled: 10*2=20
        // 2. KnockbackApplied(2, 10.0) -- crit doubled: 5*2=10
        // 3. StatSet(1, "fury_stacks", 3.0) -- stacking on_hit (crit → 3 stacks)
        // 4. StatSet(1, "attack_speed_bonus", 0.36)
        // 5. BuffAdded(1, "fury", 2.5)

        // Expected sequence of effects:
        // 1. Animate(1, "attack")
        // 2. Sound("swoosh", 1)
        // 3. Vfx("crit_particles", 2) -- is_crit=true

        assert_eq!(intents.len(), 5, "expected 5 intents, got {intents:?}");
        assert_eq!(effects.len(), 3, "expected 3 effects, got {effects:?}");

        // Effects
        assert!(
            matches!(&effects[0], Effect::Animate { entity_id: 1, animation } if animation == "attack"),
            "effects[0] should be Animate, got {:?}",
            effects[0]
        );
        assert!(
            matches!(&effects[1], Effect::Sound { name, target_id: 1 } if name == "swoosh"),
            "effects[1] should be Sound swoosh, got {:?}",
            effects[1]
        );
        assert!(
            matches!(&effects[2], Effect::Vfx { name, target_id: 2 } if name == "crit_particles"),
            "effects[2] should be Vfx crit_particles, got {:?}",
            effects[2]
        );

        // Intents
        // Crit doubled the damage: 10 * 2.0 = 20
        assert!(
            matches!(intents[0], Intent::DamageDealt { target_id: 2, amount } if (amount - 20.0).abs() < f32::EPSILON),
            "intents[0] should be DamageDealt 20.0, got {:?}",
            intents[0]
        );
        // Crit doubled the knockback: 5 * 2.0 = 10
        assert!(
            matches!(intents[1], Intent::KnockbackApplied { target_id: 2, force } if (force - 10.0).abs() < f32::EPSILON),
            "intents[1] should be KnockbackApplied 10.0, got {:?}",
            intents[1]
        );
        // Stacking on_hit intents (crit → 3 stacks)
        assert!(
            matches!(&intents[2], Intent::StatSet { entity_id: 1, stat, value }
                if stat == "fury_stacks" && (*value - 3.0).abs() < f32::EPSILON),
            "intents[2] should be StatSet fury_stacks 3, got {:?}",
            intents[2]
        );
        assert!(
            matches!(&intents[3], Intent::StatSet { entity_id: 1, stat, value }
                if stat == "attack_speed_bonus" && (*value - 0.36).abs() < 0.001),
            "intents[3] should be StatSet attack_speed_bonus 0.36, got {:?}",
            intents[3]
        );
        assert!(
            matches!(&intents[4], Intent::BuffAdded { target_id: 1, name, duration }
                if name == "fury" && (*duration - 2.5).abs() < f32::EPSILON),
            "intents[4] should be BuffAdded fury 2.5, got {:?}",
            intents[4]
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
        let registry = Arc::new(registry);

        let engine =
            ScriptEngine::new(MELEE_ATTACK_WITH_HOOKS).expect("melee attack with hooks should compile");

        let source = test_combatant(1);
        let target = Combatant {
            id: 2,
            pos_x: 1.0,
            pos_z: 0.0,
            ..test_combatant(2)
        };

        let (intents, effects) = engine
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

        // Expected intents:
        // 1. DamageDealt(2, 10.0) — unmodified
        // 2. KnockbackApplied(2, 5.0) — unmodified
        // 3. StatSet(1, fury_stacks, 1.0)
        // 4. StatSet(1, attack_speed_bonus, 0.12)
        // 5. BuffAdded(1, fury, 2.5)

        // Expected effects:
        // 1. Animate(1, "attack")
        // 2. Sound("swoosh", 1)
        // (no crit_particles since is_crit=false)

        assert_eq!(intents.len(), 5, "expected 5 intents, got {intents:?}");
        assert_eq!(effects.len(), 2, "expected 2 effects, got {effects:?}");

        assert!(
            matches!(intents[0], Intent::DamageDealt { target_id: 2, amount } if (amount - 10.0).abs() < f32::EPSILON),
            "damage should be unmodified at 10.0, got {:?}",
            intents[0]
        );
        assert!(
            matches!(intents[1], Intent::KnockbackApplied { target_id: 2, force } if (force - 5.0).abs() < f32::EPSILON),
            "knockback should be unmodified at 5.0, got {:?}",
            intents[1]
        );
        assert!(
            matches!(&intents[2], Intent::StatSet { entity_id: 1, stat, value }
                if stat == "fury_stacks" && (*value - 1.0).abs() < f32::EPSILON),
            "fury_stacks should be 1 (normal hit), got {:?}",
            intents[2]
        );
    }

    #[test]
    fn fire_hook_with_no_behaviors_passes_hit_through() {
        // No behaviors attached — fire_hook should return hit unchanged
        let registry = Arc::new(ScriptRegistry::new());
        let engine =
            ScriptEngine::new(MELEE_ATTACK_WITH_HOOKS).expect("script should compile");

        let source = test_combatant(1);
        let target = Combatant {
            id: 2,
            pos_x: 1.0,
            pos_z: 0.0,
            ..test_combatant(2)
        };

        let (intents, effects) = engine
            .call_ability_with_behaviors(
                "on_ability_start",
                source,
                vec![target],
                0.5,
                registry,
                vec![],
            )
            .expect("ability should succeed");

        // No behaviors → no modifications, no stacking intents, no crit particles
        // Intents: DamageDealt(10.0), KnockbackApplied(5.0)
        // Effects: Animate, Sound
        assert_eq!(intents.len(), 2, "expected 2 intents with no behaviors, got {intents:?}");
        assert_eq!(effects.len(), 2, "expected 2 effects with no behaviors, got {effects:?}");
        assert!(matches!(intents[0], Intent::DamageDealt { target_id: 2, amount } if (amount - 10.0).abs() < f32::EPSILON));
        assert!(matches!(intents[1], Intent::KnockbackApplied { target_id: 2, force } if (force - 5.0).abs() < f32::EPSILON));
    }

    #[test]
    fn ground_pound_with_hooks_and_crit() {
        let mut registry = ScriptRegistry::new();
        registry
            .register("crit".to_string(), CRIT_SCRIPT)
            .expect("crit should compile");
        let registry = Arc::new(registry);

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
        let (intents, effects) = engine
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
        // Expected intents: DamageDealt(80), KnockbackApplied(40)
        // Expected effects: Animate, Sound, Vfx(shockwave), ScreenShake
        assert_eq!(intents.len(), 2, "expected 2 intents, got {intents:?}");
        assert_eq!(effects.len(), 4, "expected 4 effects, got {effects:?}");
        assert!(
            matches!(intents[0], Intent::DamageDealt { target_id: 2, amount } if (amount - 80.0).abs() < f32::EPSILON),
            "ground pound crit damage should be 80, got {:?}",
            intents[0]
        );
        assert!(
            matches!(intents[1], Intent::KnockbackApplied { target_id: 2, force } if (force - 40.0).abs() < f32::EPSILON),
            "ground pound crit knockback should be 40, got {:?}",
            intents[1]
        );
    }
}

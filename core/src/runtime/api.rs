use std::cell::RefCell;
use std::sync::Arc;

use rune::runtime::Vm;
use rune::{ContextError, Module};

use super::commands::{Command, CommandBuffer};
use super::registry::ScriptRegistry;
use super::types::{Combatant, Hit};

/// An intent records a state-changing action during script execution.
#[derive(Debug, Clone)]
pub enum Intent {
    DamageDealt { target_id: u64, amount: f32 },
    Healed { target_id: u64, amount: f32 },
    KnockbackApplied { target_id: u64, force: f32 },
    BuffAdded { target_id: u64, name: String, duration: f32 },
    BuffRemoved { target_id: u64, name: String },
    StatSet { entity_id: u64, stat: String, value: f32 },
    Killed { target_id: u64 },
    BehaviorSet { entity_id: u64, behavior: String },
    MovedToward { entity_id: u64, target_x: f32, target_z: f32, speed: f32 },
}

/// A presentation effect. Client processes these; server ignores.
#[derive(Debug, Clone)]
pub enum Effect {
    Vfx { name: String, target_id: u64 },
    Sound { name: String, target_id: u64 },
    ScreenShake { intensity: f32 },
    HitStop { duration: f32 },
    Animate { entity_id: u64, animation: String },
}

thread_local! {
    static COMMAND_BUFFER: RefCell<CommandBuffer> = RefCell::new(CommandBuffer::new());
    static RNG_ROLL: RefCell<f32> = RefCell::new(0.0);
    static AVAILABLE_TARGETS: RefCell<Vec<Combatant>> = RefCell::new(Vec::new());
    static AVAILABLE_PLAYERS: RefCell<Vec<Combatant>> = RefCell::new(Vec::new());
    static ENTITY_BEHAVIORS: RefCell<Vec<String>> = RefCell::new(Vec::new());
    static SCRIPT_REGISTRY: RefCell<Option<Arc<ScriptRegistry>>> = RefCell::new(None);
    static INTENT_LOG: RefCell<Vec<Intent>> = RefCell::new(Vec::new());
    static EFFECT_LOG: RefCell<Vec<Effect>> = RefCell::new(Vec::new());
}

/// Set the RNG roll value before calling a script.
pub fn set_rng_roll(roll: f32) {
    RNG_ROLL.with(|r| *r.borrow_mut() = roll);
}

/// Set the available targets before calling an ability script.
pub fn set_available_targets(targets: Vec<Combatant>) {
    AVAILABLE_TARGETS.with(|t| *t.borrow_mut() = targets);
}

/// Set the available players before calling a tick script.
pub fn set_available_players(players: Vec<Combatant>) {
    AVAILABLE_PLAYERS.with(|p| *p.borrow_mut() = players);
}

/// Set the list of behavior script names attached to the current entity.
pub fn set_entity_behaviors(behaviors: Vec<String>) {
    ENTITY_BEHAVIORS.with(|b| *b.borrow_mut() = behaviors);
}

/// Set the script registry for `fire_hook` to use during ability execution.
pub fn set_script_registry(registry: Arc<ScriptRegistry>) {
    SCRIPT_REGISTRY.with(|r| *r.borrow_mut() = Some(registry));
}

/// Clear the script registry after ability execution.
pub fn clear_script_registry() {
    SCRIPT_REGISTRY.with(|r| *r.borrow_mut() = None);
}

/// Drain all commands emitted by the last script call.
pub fn take_commands() -> Vec<Command> {
    COMMAND_BUFFER.with(|buf| buf.borrow_mut().drain())
}

fn push_command(cmd: Command) {
    COMMAND_BUFFER.with(|buf| buf.borrow_mut().push(cmd));
}

pub fn push_intent(intent: Intent) {
    INTENT_LOG.with(|log| log.borrow_mut().push(intent));
}

pub fn push_effect(effect: Effect) {
    EFFECT_LOG.with(|log| log.borrow_mut().push(effect));
}

pub fn take_intents() -> Vec<Intent> {
    INTENT_LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

pub fn take_effects() -> Vec<Effect> {
    EFFECT_LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

pub fn clear_logs() {
    let _ = take_intents();
    let _ = take_effects();
}

/// Build the `gameplay` module that Rune scripts import via `use gameplay::*;`.
pub fn build_gameplay_module() -> Result<Module, ContextError> {
    let mut m = Module::with_crate("gameplay")?;

    // Register types so Rune knows about Combatant and Hit.
    m.ty::<Combatant>()?;
    m.ty::<Hit>()?;

    // Command functions (buffered writes)
    m.function("damage", damage).build()?;
    m.function("heal", heal).build()?;
    m.function("knockback", knockback).build()?;
    m.function("vfx", vfx).build()?;
    m.function("sound", sound).build()?;
    m.function("animate", animate).build()?;
    m.function("buff", buff).build()?;
    m.function("remove_buff", remove_buff).build()?;
    m.function("set_stat", set_stat).build()?;
    m.function("screen_shake", screen_shake).build()?;
    m.function("hit_stop", hit_stop).build()?;
    m.function("set_behavior", set_behavior).build()?;
    m.function("move_toward", move_toward).build()?;

    // Query functions (reads)
    m.function("chance", chance).build()?;
    m.function("distance_2d", distance_2d).build()?;
    m.function("min", min_f32).build()?;
    m.function("max", max_f32).build()?;

    // Target query functions (for ability scripts)
    m.function("targets_in_cone", targets_in_cone).build()?;
    m.function("targets_in_radius", targets_in_radius).build()?;

    // AI query functions
    m.function("nearest_player", nearest_player).build()?;

    // Hook chaining
    m.function("fire_hook", fire_hook).build()?;

    Ok(m)
}

// --- Command functions ---

fn damage(target: &Combatant, amount: f32) {
    push_command(Command::DealDamage {
        target_id: target.id,
        amount,
    });
}

fn heal(target: &Combatant, amount: f32) {
    push_command(Command::Heal {
        target_id: target.id,
        amount,
    });
}

fn knockback(target: &Combatant, force: f32) {
    push_command(Command::ApplyKnockback {
        target_id: target.id,
        force,
    });
}

fn vfx(name: &str, target: &Combatant) {
    push_command(Command::SpawnVfx {
        name: name.to_string(),
        target_id: target.id,
    });
}

fn sound(name: &str, x: f32, y: f32, z: f32) {
    push_command(Command::PlaySound {
        name: name.to_string(),
        pos_x: x,
        pos_y: y,
        pos_z: z,
    });
}

fn animate(entity: &Combatant, animation: &str) {
    push_command(Command::Animate {
        entity_id: entity.id,
        animation: animation.to_string(),
    });
}

fn buff(target: &Combatant, name: &str, duration: f32) {
    push_command(Command::AddBuff {
        target_id: target.id,
        name: name.to_string(),
        duration,
    });
}

fn remove_buff(target: &Combatant, name: &str) {
    push_command(Command::RemoveBuff {
        target_id: target.id,
        name: name.to_string(),
    });
}

fn set_stat(entity: &Combatant, stat: &str, value: f32) {
    push_command(Command::SetStat {
        entity_id: entity.id,
        stat: stat.to_string(),
        value,
    });
}

fn screen_shake(intensity: f32) {
    push_command(Command::ScreenShake { intensity });
}

fn hit_stop(duration: f32) {
    push_command(Command::HitStop { duration });
}

fn set_behavior(entity: &Combatant, behavior: &str) {
    push_command(Command::SetBehavior {
        entity_id: entity.id,
        behavior: behavior.to_string(),
    });
}

fn move_toward(entity: &Combatant, target_x: f32, target_z: f32, speed: f32) {
    push_command(Command::MoveToward {
        entity_id: entity.id,
        target_x,
        target_z,
        speed,
    });
}

// --- Query functions ---

fn chance(probability: f32) -> bool {
    RNG_ROLL.with(|r| *r.borrow() < probability)
}

fn distance_2d(a: &Combatant, b: &Combatant) -> f32 {
    let dx = a.pos_x - b.pos_x;
    let dz = a.pos_z - b.pos_z;
    (dx * dx + dz * dz).sqrt()
}

fn min_f32(a: f32, b: f32) -> f32 {
    a.min(b)
}

fn max_f32(a: f32, b: f32) -> f32 {
    a.max(b)
}

/// Chain a hook through all entity behaviors that implement it.
///
/// Called from Rune as `fire_hook("on_pre_hit", source, target, hit)`.
/// For each behavior script attached to the entity that has the named function,
/// calls it with (source, target, hit) and threads the Hit through.
fn fire_hook(hook_name: &str, source: &Combatant, target: &Combatant, hit: Hit) -> Hit {
    let behaviors = ENTITY_BEHAVIORS.with(|b| b.borrow().clone());

    let registry = SCRIPT_REGISTRY.with(|r| r.borrow().clone());
    let Some(registry) = registry else {
        return hit;
    };

    let mut current_hit = hit;

    for behavior_name in &behaviors {
        let Some(engine) = registry.get(behavior_name) else {
            continue;
        };
        if !engine.has_function(hook_name) {
            continue;
        }

        // Create a fresh VM and call the hook. The COMMAND_BUFFER and RNG_ROLL
        // thread-locals are shared, so commands from behavior scripts accumulate
        // alongside the ability script's commands.
        let mut vm = Vm::new(engine.runtime.clone(), engine.unit.clone());
        match vm.call(
            [hook_name],
            (source.clone(), target.clone(), current_hit.clone()),
        ) {
            Ok(output) => {
                if let Ok(new_hit) = rune::from_value::<Hit>(output) {
                    current_hit = new_hit;
                }
            }
            Err(_) => {
                // If a behavior hook fails, skip it and continue with the current hit.
                continue;
            }
        }
    }

    current_hit
}

fn nearest_player(pos_x: f32, pos_z: f32) -> Option<Combatant> {
    AVAILABLE_PLAYERS.with(|players| {
        let players = players.borrow();
        let mut closest: Option<(f32, &Combatant)> = None;
        for p in players.iter() {
            let dx = pos_x - p.pos_x;
            let dz = pos_z - p.pos_z;
            let dist_sq = dx * dx + dz * dz;
            match closest {
                Some((best_dist_sq, _)) if dist_sq < best_dist_sq => {
                    closest = Some((dist_sq, p));
                }
                None => {
                    closest = Some((dist_sq, p));
                }
                _ => {}
            }
        }
        closest.map(|(_, c)| c.clone())
    })
}

fn targets_in_cone(
    source: &Combatant,
    range: f32,
    arc: f32,
) -> rune::runtime::Vec {
    let origin = glam::Vec2::new(source.pos_x, source.pos_z);
    let forward = glam::Vec2::new(source.dir_x, source.dir_z).normalize_or_zero();
    let half_arc_cos = (arc.to_radians() / 2.0).cos();

    let mut result = rune::runtime::Vec::new();
    AVAILABLE_TARGETS.with(|targets| {
        for t in targets.borrow().iter() {
            let target_pos = glam::Vec2::new(t.pos_x, t.pos_z);
            if crate::combat::cone_hit_check(origin, forward, target_pos, range, half_arc_cos) {
                let _ = result.push_value(t.clone());
            }
        }
    });
    result
}

fn targets_in_radius(pos_x: f32, pos_z: f32, radius: f32) -> rune::runtime::Vec {
    let origin = glam::Vec2::new(pos_x, pos_z);
    let radius_sq = radius * radius;

    let mut result = rune::runtime::Vec::new();
    AVAILABLE_TARGETS.with(|targets| {
        for t in targets.borrow().iter() {
            let target_pos = glam::Vec2::new(t.pos_x, t.pos_z);
            if origin.distance_squared(target_pos) <= radius_sq {
                let _ = result.push_value(t.clone());
            }
        }
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gameplay_module_builds() {
        build_gameplay_module().expect("gameplay module should build without error");
    }

    #[test]
    fn intent_log_collects_intents() {
        clear_logs();
        push_intent(Intent::DamageDealt { target_id: 1, amount: 50.0 });
        push_intent(Intent::Healed { target_id: 2, amount: 25.0 });
        let intents = take_intents();
        assert_eq!(intents.len(), 2);
    }

    #[test]
    fn effect_log_collects_effects() {
        clear_logs();
        push_effect(Effect::Vfx { name: "slash".into(), target_id: 1 });
        push_effect(Effect::Sound { name: "hit".into(), target_id: 1 });
        let effects = take_effects();
        assert_eq!(effects.len(), 2);
    }

    #[test]
    fn clear_logs_empties_both() {
        push_intent(Intent::Killed { target_id: 1 });
        push_effect(Effect::ScreenShake { intensity: 0.5 });
        clear_logs();
        assert!(take_intents().is_empty());
        assert!(take_effects().is_empty());
    }
}

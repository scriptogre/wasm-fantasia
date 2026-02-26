use std::cell::RefCell;
use std::rc::Rc;

use rune::runtime::Vm;
use rune::{ContextError, Module};

use super::commands::{Command, CommandBuffer};
use super::registry::ScriptRegistry;
use super::types::{Combatant, Hit};

thread_local! {
    static COMMAND_BUFFER: RefCell<CommandBuffer> = RefCell::new(CommandBuffer::new());
    static RNG_ROLL: RefCell<f32> = RefCell::new(0.0);
    static AVAILABLE_TARGETS: RefCell<Vec<Combatant>> = RefCell::new(Vec::new());
    static ENTITY_BEHAVIORS: RefCell<Vec<String>> = RefCell::new(Vec::new());
    static SCRIPT_REGISTRY: RefCell<Option<Rc<ScriptRegistry>>> = RefCell::new(None);
}

/// Set the RNG roll value before calling a script.
pub fn set_rng_roll(roll: f32) {
    RNG_ROLL.with(|r| *r.borrow_mut() = roll);
}

/// Set the available targets before calling an ability script.
pub fn set_available_targets(targets: Vec<Combatant>) {
    AVAILABLE_TARGETS.with(|t| *t.borrow_mut() = targets);
}

/// Set the list of behavior script names attached to the current entity.
pub fn set_entity_behaviors(behaviors: Vec<String>) {
    ENTITY_BEHAVIORS.with(|b| *b.borrow_mut() = behaviors);
}

/// Set the script registry for `fire_hook` to use during ability execution.
pub fn set_script_registry(registry: Rc<ScriptRegistry>) {
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

/// Build the `game` module that Rune scripts import via `use game::*;`.
pub fn build_game_module() -> Result<Module, ContextError> {
    let mut m = Module::with_crate("game")?;

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
    fn game_module_builds() {
        build_game_module().expect("game module should build without error");
    }
}

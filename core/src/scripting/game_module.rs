use std::cell::RefCell;

use rune::{ContextError, Module};

use super::commands::{Command, CommandBuffer};
use super::types::{Combatant, Hit};

thread_local! {
    static COMMAND_BUFFER: RefCell<CommandBuffer> = RefCell::new(CommandBuffer::new());
    static RNG_ROLL: RefCell<f32> = RefCell::new(0.0);
    static AVAILABLE_TARGETS: RefCell<Vec<Combatant>> = RefCell::new(Vec::new());
}

/// Set the RNG roll value before calling a script.
pub fn set_rng_roll(roll: f32) {
    RNG_ROLL.with(|r| *r.borrow_mut() = roll);
}

/// Set the available targets before calling an ability script.
pub fn set_available_targets(targets: Vec<Combatant>) {
    AVAILABLE_TARGETS.with(|t| *t.borrow_mut() = targets);
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

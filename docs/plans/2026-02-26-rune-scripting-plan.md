# Rune Scripting System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace hardcoded Rust combat logic and enemy AI with Rune scripts, establishing Rune as the primary gameplay scripting language.

**Architecture:** Rune engine lives in `core/` crate. Scripts define gameplay behaviors (crit, stacking, abilities, AI) as composable `.rune` files. Both client and server run scripts — client for immediate feel, server for authority. Scripts emit commands via a buffer; platform-specific executors apply them to ECS or SpacetimeDB tables.

**Tech Stack:** Rune 0.14.1 (bytecode VM, pure Rust), Bevy 0.18, SpacetimeDB 1.12.0

**Design doc:** `docs/plans/2026-02-26-rune-scripting-design.md`

---

## Task 1: Add Rune to core/ and verify WASM compilation

**Files:**
- Modify: `core/Cargo.toml`
- Modify: `Cargo.toml` (workspace)
- Create: `core/src/scripting.rs`
- Modify: `core/src/lib.rs`

**Step 1: Add rune dependency to core/**

In `core/Cargo.toml`, add under `[dependencies]`:
```toml
rune = "0.14.1"
```

**Step 2: Create minimal scripting module**

Create `core/src/scripting.rs`:
```rust
use rune::vm::VmError;
use rune::{Context, Diagnostics, Source, Sources, Unit, Vm};
use std::sync::Arc;

pub struct ScriptEngine {
    context: Context,
    unit: Arc<Unit>,
}

impl ScriptEngine {
    /// Compile a Rune script from source.
    pub fn compile(script: &str) -> Result<Self, String> {
        let context = Context::with_default_modules().map_err(|e| e.to_string())?;

        let mut sources = Sources::new();
        sources.insert(Source::memory(script).map_err(|e| e.to_string())?);

        let mut diagnostics = Diagnostics::new();
        let unit = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build()
            .map_err(|_| format!("Compilation failed: {diagnostics:#?}"))?;

        Ok(Self {
            context,
            unit: Arc::new(unit),
        })
    }

    /// Call a function by name with no arguments, return the result.
    pub fn call_void(&self, function: &str) -> Result<(), String> {
        let mut vm = Vm::new(
            Arc::new(self.context.runtime().map_err(|e| e.to_string())?),
            self.unit.clone(),
        );
        vm.call([function], ())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_and_run_trivial_script() {
        let engine = ScriptEngine::compile(
            r#"
            pub fn hello() {
                let x = 1 + 2;
            }
            "#,
        )
        .expect("should compile");
        engine.call_void("hello").expect("should run");
    }
}
```

**Step 3: Export module from lib.rs**

In `core/src/lib.rs`, add:
```rust
pub mod scripting;
```

**Step 4: Run tests**

Run: `cargo test -p game-core`
Expected: PASS — trivial script compiles and runs.

**Step 5: Verify WASM compilation**

Run: `cargo check -p game-core --target wasm32-unknown-unknown`
Expected: PASS — Rune is pure Rust, should compile to WASM. If this fails, we need to investigate Rune's WASM compatibility early.

**Step 6: Commit**

```bash
git add core/Cargo.toml core/src/scripting.rs core/src/lib.rs
git commit -m "Add Rune scripting engine to core crate"
```

---

## Task 2: Define the command buffer and game types

**Files:**
- Create: `core/src/scripting/mod.rs` (rename from `core/src/scripting.rs`)
- Create: `core/src/scripting/commands.rs`
- Create: `core/src/scripting/types.rs`

**Step 1: Restructure scripting as a module directory**

Move `core/src/scripting.rs` to `core/src/scripting/mod.rs`. Add:
```rust
pub mod commands;
pub mod types;
```

**Step 2: Define game types exposed to Rune**

Create `core/src/scripting/types.rs`:
```rust
use rune::Any;

/// A combatant as seen by scripts. Flattened stats for ergonomic field access.
#[derive(Any, Debug, Clone)]
#[rune(constructor)]
pub struct Combatant {
    #[rune(get, set)]
    pub id: u64,
    #[rune(get, set)]
    pub pos_x: f32,
    #[rune(get, set)]
    pub pos_y: f32,
    #[rune(get, set)]
    pub pos_z: f32,
    #[rune(get, set)]
    pub dir_x: f32,
    #[rune(get, set)]
    pub dir_z: f32,
    #[rune(get, set)]
    pub health: f32,
    #[rune(get, set)]
    pub max_health: f32,
    // Offensive
    #[rune(get, set)]
    pub attack_damage: f32,
    #[rune(get, set)]
    pub crit_chance: f32,
    #[rune(get, set)]
    pub crit_multiplier: f32,
    #[rune(get, set)]
    pub knockback_force: f32,
    #[rune(get, set)]
    pub attack_range: f32,
    #[rune(get, set)]
    pub attack_arc: f32,
    #[rune(get, set)]
    pub attack_speed: f32,
    // Dynamic
    #[rune(get, set)]
    pub fury_stacks: i64,
    #[rune(get, set)]
    pub attack_speed_bonus: f32,
    // AI
    #[rune(get, set)]
    pub cooldown_ready: bool,
    #[rune(get, set)]
    pub speed: f32,
}

/// Per-hit context. Mutable — scripts modify this during on_pre_hit.
#[derive(Any, Debug, Clone)]
#[rune(constructor)]
pub struct Hit {
    #[rune(get, set)]
    pub damage: f32,
    #[rune(get, set)]
    pub knockback: f32,
    #[rune(get, set)]
    pub is_crit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_default_values() {
        let hit = Hit {
            damage: 25.0,
            knockback: 6.0,
            is_crit: false,
        };
        assert_eq!(hit.damage, 25.0);
        assert!(!hit.is_crit);
    }
}
```

**Step 3: Define command buffer**

Create `core/src/scripting/commands.rs`:
```rust
/// Commands emitted by scripts. Applied by platform-specific executors after script returns.
#[derive(Debug, Clone)]
pub enum Command {
    DealDamage {
        target_id: u64,
        amount: f32,
    },
    Heal {
        target_id: u64,
        amount: f32,
    },
    ApplyKnockback {
        target_id: u64,
        force: f32,
    },
    SpawnVfx {
        name: String,
        target_id: u64,
    },
    PlaySound {
        name: String,
        pos_x: f32,
        pos_y: f32,
        pos_z: f32,
    },
    Animate {
        entity_id: u64,
        animation: String,
    },
    AddBuff {
        target_id: u64,
        name: String,
        duration: f32,
    },
    RemoveBuff {
        target_id: u64,
        name: String,
    },
    SetStat {
        entity_id: u64,
        stat: String,
        value: f32,
    },
    SetBehavior {
        entity_id: u64,
        behavior: String,
    },
    MoveToward {
        entity_id: u64,
        target_x: f32,
        target_z: f32,
        speed: f32,
    },
    ScreenShake {
        intensity: f32,
    },
    HitStop {
        duration: f32,
    },
}

/// Collects commands emitted during script execution.
#[derive(Debug, Default, Clone)]
pub struct CommandBuffer {
    pub commands: Vec<Command>,
}

impl CommandBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, cmd: Command) {
        self.commands.push(cmd);
    }

    pub fn drain(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_buffer_collects_and_drains() {
        let mut buf = CommandBuffer::new();
        buf.push(Command::DealDamage {
            target_id: 1,
            amount: 50.0,
        });
        buf.push(Command::SpawnVfx {
            name: "crit".into(),
            target_id: 1,
        });
        let cmds = buf.drain();
        assert_eq!(cmds.len(), 2);
        assert!(buf.commands.is_empty());
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p game-core`
Expected: PASS

**Step 5: Commit**

```bash
git add core/src/scripting/
git commit -m "Define command buffer and script types for Rune"
```

---

## Task 3: Register game module with Rune and wire up command functions

**Files:**
- Modify: `core/src/scripting/mod.rs`
- Create: `core/src/scripting/game_module.rs`

**Step 1: Create the game module that scripts `use game::*` from**

Create `core/src/scripting/game_module.rs`. This registers all functions that scripts can call. The command buffer is shared via Rune's `Shared` wrapper.

```rust
use crate::scripting::commands::{Command, CommandBuffer};
use crate::scripting::types::{Combatant, Hit};
use rune::runtime::Shared;
use rune::{ContextError, Module};
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static COMMAND_BUFFER: RefCell<CommandBuffer> = RefCell::new(CommandBuffer::new());
    static RNG_ROLL: RefCell<f32> = RefCell::new(0.0);
}

pub fn set_rng_roll(roll: f32) {
    RNG_ROLL.with(|r| *r.borrow_mut() = roll);
}

pub fn take_commands() -> Vec<Command> {
    COMMAND_BUFFER.with(|buf| buf.borrow_mut().drain())
}

fn push_command(cmd: Command) {
    COMMAND_BUFFER.with(|buf| buf.borrow_mut().push(cmd));
}

/// Build the `game` module for Rune scripts.
pub fn build_game_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("game")?;

    // Register types
    module.ty::<Combatant>()?;
    module.ty::<Hit>()?;

    // --- Command functions (buffered writes) ---

    module.function("damage", |target: &Combatant, amount: f32| {
        push_command(Command::DealDamage {
            target_id: target.id,
            amount,
        });
    })?;

    module.function("heal", |target: &Combatant, amount: f32| {
        push_command(Command::Heal {
            target_id: target.id,
            amount,
        });
    })?;

    module.function("knockback", |target: &Combatant, force: f32| {
        push_command(Command::ApplyKnockback {
            target_id: target.id,
            force,
        });
    })?;

    module.function("vfx", |name: &str, target: &Combatant| {
        push_command(Command::SpawnVfx {
            name: name.to_string(),
            target_id: target.id,
        });
    })?;

    module.function(
        "sound",
        |name: &str, x: f32, y: f32, z: f32| {
            push_command(Command::PlaySound {
                name: name.to_string(),
                pos_x: x,
                pos_y: y,
                pos_z: z,
            });
        },
    )?;

    module.function("animate", |entity: &Combatant, animation: &str| {
        push_command(Command::Animate {
            entity_id: entity.id,
            animation: animation.to_string(),
        });
    })?;

    module.function(
        "buff",
        |target: &Combatant, name: &str, duration: f32| {
            push_command(Command::AddBuff {
                target_id: target.id,
                name: name.to_string(),
                duration,
            });
        },
    )?;

    module.function("remove_buff", |target: &Combatant, name: &str| {
        push_command(Command::RemoveBuff {
            target_id: target.id,
            name: name.to_string(),
        });
    })?;

    module.function(
        "set_stat",
        |entity: &Combatant, stat: &str, value: f32| {
            push_command(Command::SetStat {
                entity_id: entity.id,
                stat: stat.to_string(),
                value,
            });
        },
    )?;

    module.function("screen_shake", |intensity: f32| {
        push_command(Command::ScreenShake { intensity });
    })?;

    module.function("hit_stop", |duration: f32| {
        push_command(Command::HitStop { duration });
    })?;

    module.function("set_behavior", |entity: &Combatant, behavior: &str| {
        push_command(Command::SetBehavior {
            entity_id: entity.id,
            behavior: behavior.to_string(),
        });
    })?;

    module.function(
        "move_toward",
        |entity: &Combatant, target_x: f32, target_z: f32, speed: f32| {
            push_command(Command::MoveToward {
                entity_id: entity.id,
                target_x,
                target_z,
                speed,
            });
        },
    )?;

    // --- Query functions (reads) ---

    module.function("chance", |probability: f32| -> bool {
        RNG_ROLL.with(|r| *r.borrow() < probability)
    })?;

    module.function("distance_2d", |a: &Combatant, b: &Combatant| -> f32 {
        let dx = a.pos_x - b.pos_x;
        let dz = a.pos_z - b.pos_z;
        (dx * dx + dz * dz).sqrt()
    })?;

    module.function(
        "min",
        |a: f32, b: f32| -> f32 { a.min(b) },
    )?;

    module.function(
        "max",
        |a: f32, b: f32| -> f32 { a.max(b) },
    )?;

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_module_builds() {
        build_game_module().expect("game module should build without errors");
    }
}
```

**Step 2: Update scripting/mod.rs to use game module**

Update `core/src/scripting/mod.rs`:
```rust
pub mod commands;
pub mod game_module;
pub mod types;

use crate::scripting::commands::Command;
use crate::scripting::game_module::{build_game_module, set_rng_roll, take_commands};
use crate::scripting::types::{Combatant, Hit};
use rune::{Context, Diagnostics, Source, Sources, Unit, Vm};
use std::sync::Arc;

pub struct ScriptEngine {
    runtime: Arc<rune::runtime::RuntimeContext>,
    unit: Arc<Unit>,
}

impl ScriptEngine {
    /// Compile a Rune script with the game module available.
    pub fn compile(script: &str) -> Result<Self, String> {
        let mut context = Context::with_default_modules().map_err(|e| e.to_string())?;
        context
            .install(build_game_module().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;

        let mut sources = Sources::new();
        sources.insert(Source::memory(script).map_err(|e| e.to_string())?);

        let mut diagnostics = Diagnostics::new();
        let unit = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build()
            .map_err(|_| format!("Compilation failed: {diagnostics:#?}"))?;

        let runtime = Arc::new(context.runtime().map_err(|e| e.to_string())?);

        Ok(Self {
            runtime,
            unit: Arc::new(unit),
        })
    }

    /// Check if a function exists in the script.
    pub fn has_function(&self, name: &str) -> bool {
        self.unit.function(rune::Hash::type_hash([name])).is_some()
    }

    /// Call a hook with (source, target, hit) args. Returns commands emitted.
    pub fn call_hit_hook(
        &self,
        function: &str,
        source: Combatant,
        target: Combatant,
        hit: Hit,
        rng_roll: f32,
    ) -> Result<(Hit, Vec<Command>), String> {
        set_rng_roll(rng_roll);

        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        let output = vm
            .call([function], (source, target, hit.clone()))
            .map_err(|e| e.to_string())?;

        let commands = take_commands();
        // The hit may have been mutated by the script
        // For now, we return the original — Task 4 will handle mutable hit passing
        Ok((hit, commands))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_script_with_game_module() {
        let engine = ScriptEngine::compile(
            r#"
            use game::*;

            pub fn on_hit(source, target, hit) {
                damage(target, hit.damage);
            }
            "#,
        )
        .expect("should compile");
        assert!(engine.has_function("on_hit"));
    }

    #[test]
    fn script_emits_commands() {
        let engine = ScriptEngine::compile(
            r#"
            use game::*;

            pub fn on_hit(source, target, hit) {
                damage(target, 50.0);
                vfx("sparks", target);
            }
            "#,
        )
        .expect("should compile");

        let source = Combatant {
            id: 0, pos_x: 0.0, pos_y: 0.0, pos_z: 0.0,
            dir_x: 0.0, dir_z: 1.0, health: 100.0, max_health: 100.0,
            attack_damage: 25.0, crit_chance: 0.2, crit_multiplier: 2.5,
            knockback_force: 6.0, attack_range: 3.6, attack_arc: 150.0,
            attack_speed: 1.0, fury_stacks: 0, attack_speed_bonus: 0.0,
            cooldown_ready: true, speed: 2.0,
        };
        let target = Combatant { id: 1, ..source.clone() };
        let hit = Hit { damage: 25.0, knockback: 6.0, is_crit: false };

        let (_, commands) = engine
            .call_hit_hook("on_hit", source, target, hit, 0.5)
            .expect("should run");

        assert_eq!(commands.len(), 2);
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p game-core`
Expected: PASS

**Step 4: Commit**

```bash
git add core/src/scripting/
git commit -m "Register game module with Rune and wire up command functions"
```

---

## Task 4: Write the crit behavior script and integration test

This is the first real script. We port `core/src/presets/crit.rs` to Rune.

**Files:**
- Create: `client/assets/scripts/behaviors/crit.rune`
- Modify: `core/src/scripting/mod.rs` (add test)

**Step 1: Write the crit script**

Create `client/assets/scripts/behaviors/crit.rune`:
```rune
use game::*;

pub fn on_pre_hit(source, target, hit) {
    if chance(source.crit_chance) {
        hit.is_crit = true;
        hit.damage *= source.crit_multiplier;
        hit.knockback *= source.crit_multiplier;
    }
}
```

**Step 2: Write integration test in core that loads and runs the script**

Add test to `core/src/scripting/mod.rs`:
```rust
#[test]
fn crit_script_modifies_hit_on_low_roll() {
    let engine = ScriptEngine::compile(
        r#"
        use game::*;

        pub fn on_pre_hit(source, target, hit) {
            if chance(source.crit_chance) {
                hit.is_crit = true;
                hit.damage *= source.crit_multiplier;
                hit.knockback *= source.crit_multiplier;
            }
        }
        "#,
    )
    .expect("should compile");

    let source = Combatant {
        id: 0, pos_x: 0.0, pos_y: 0.0, pos_z: 0.0,
        dir_x: 0.0, dir_z: 1.0, health: 100.0, max_health: 100.0,
        attack_damage: 25.0, crit_chance: 0.2, crit_multiplier: 2.5,
        knockback_force: 6.0, attack_range: 3.6, attack_arc: 150.0,
        attack_speed: 1.0, fury_stacks: 0, attack_speed_bonus: 0.0,
        cooldown_ready: true, speed: 2.0,
    };
    let target = Combatant { id: 1, ..source.clone() };
    let hit = Hit { damage: 25.0, knockback: 6.0, is_crit: false };

    // Roll 0.1 < crit_chance 0.2 → should crit
    let (result_hit, _) = engine
        .call_hit_hook("on_pre_hit", source, target, hit, 0.1)
        .expect("should run");

    assert!(result_hit.is_crit);
    assert_eq!(result_hit.damage, 62.5); // 25 * 2.5
    assert_eq!(result_hit.knockback, 15.0); // 6 * 2.5
}

#[test]
fn crit_script_no_crit_on_high_roll() {
    let engine = ScriptEngine::compile(
        r#"
        use game::*;

        pub fn on_pre_hit(source, target, hit) {
            if chance(source.crit_chance) {
                hit.is_crit = true;
                hit.damage *= source.crit_multiplier;
                hit.knockback *= source.crit_multiplier;
            }
        }
        "#,
    )
    .expect("should compile");

    let source = Combatant {
        id: 0, pos_x: 0.0, pos_y: 0.0, pos_z: 0.0,
        dir_x: 0.0, dir_z: 1.0, health: 100.0, max_health: 100.0,
        attack_damage: 25.0, crit_chance: 0.2, crit_multiplier: 2.5,
        knockback_force: 6.0, attack_range: 3.6, attack_arc: 150.0,
        attack_speed: 1.0, fury_stacks: 0, attack_speed_bonus: 0.0,
        cooldown_ready: true, speed: 2.0,
    };
    let target = Combatant { id: 1, ..source.clone() };
    let hit = Hit { damage: 25.0, knockback: 6.0, is_crit: false };

    // Roll 0.9 > crit_chance 0.2 → should not crit
    let (result_hit, _) = engine
        .call_hit_hook("on_pre_hit", source, target, hit, 0.9)
        .expect("should run");

    assert!(!result_hit.is_crit);
    assert_eq!(result_hit.damage, 25.0);
}
```

**Note:** These tests will likely fail initially because `call_hit_hook` doesn't yet return the mutated `Hit`. The script mutates `hit` in-place, but Rune passes by value. We need to handle this — either by having the script return the hit, or by using Rune's `Shared` wrapper. Adjust `call_hit_hook` implementation as needed to make the test pass. The key contract is: scripts can mutate `hit` and the caller sees the changes.

**Step 3: Run tests, iterate on hit mutation**

Run: `cargo test -p game-core`
Fix `call_hit_hook` until the crit test passes. This may require:
- Having the script return the modified hit: `pub fn on_pre_hit(source, target, hit) { ... hit }` and reading the return value
- Or using Rune's `Shared<Hit>` so mutations propagate

**Step 4: Commit**

```bash
git add client/assets/scripts/behaviors/crit.rune core/src/scripting/
git commit -m "Implement crit behavior as Rune script with tests"
```

---

## Task 5: Write the stacking behavior script

Port `core/src/presets/stacking.rs` to Rune.

**Files:**
- Create: `client/assets/scripts/behaviors/stacking.rune`
- Modify: `core/src/scripting/mod.rs` (add test)

**Step 1: Write the stacking script**

Create `client/assets/scripts/behaviors/stacking.rune`:
```rune
use game::*;

pub fn on_hit(source, target, hit) {
    let add = if hit.is_crit { 3 } else { 1 };
    let stacks = min(source.fury_stacks + add, 12);
    set_stat(source, "fury_stacks", stacks);
    set_stat(source, "attack_speed_bonus", stacks * 0.12);
    buff(source, "fury", 2.5);
}

pub fn on_buff_expired(source, buff_name) {
    if buff_name == "fury" {
        set_stat(source, "fury_stacks", 0);
        set_stat(source, "attack_speed_bonus", 0.0);
    }
}
```

**Step 2: Write tests**

Add tests verifying:
- `on_hit` with `is_crit = false` emits `SetStat(fury_stacks, 1)`, `SetStat(attack_speed_bonus, 0.12)`, `AddBuff(fury, 2.5)`
- `on_hit` with `is_crit = true` emits `SetStat(fury_stacks, 3)`, `SetStat(attack_speed_bonus, 0.36)`
- Stacks cap at 12

**Step 3: Run tests**

Run: `cargo test -p game-core`
Expected: PASS

**Step 4: Commit**

```bash
git add client/assets/scripts/behaviors/stacking.rune core/src/scripting/
git commit -m "Implement stacking behavior as Rune script"
```

---

## Task 6: Write the feedback behavior script

Port `core/src/presets/feedback.rs` to Rune.

**Files:**
- Create: `client/assets/scripts/behaviors/feedback.rune`
- Modify: `core/src/scripting/mod.rs` (add test)

**Step 1: Write the feedback script**

Create `client/assets/scripts/behaviors/feedback.rune`:
```rune
use game::*;

pub fn on_hit(source, target, hit) {
    let intensity = if hit.is_crit { 1.0 } else { 0.5 };
    vfx("hit_flash", target);
    sound("impact", target.pos_x, target.pos_y, target.pos_z);
    screen_shake(intensity);
    hit_stop(if hit.is_crit { 0.08 } else { 0.04 });
}
```

**Step 2: Write test verifying it emits correct commands for crit vs non-crit**

**Step 3: Run tests, commit**

```bash
git add client/assets/scripts/behaviors/feedback.rune core/src/scripting/
git commit -m "Implement feedback behavior as Rune script"
```

---

## Task 7: Write melee_attack ability script

Port the combat resolution from `core/src/combat.rs` `resolve_combat()` and `client/src/combat/attack.rs` `on_attack_hit()`.

**Files:**
- Create: `client/assets/scripts/abilities/melee_attack.rune`
- Modify: `core/src/scripting/mod.rs` (add `call_ability` method and `targets_in_cone` support)

**Step 1: Add targets_in_cone to game module**

The `targets_in_cone` function needs a list of potential targets passed in. Add a way to pass target lists to the script context. Options:
- Pass targets as a Rune `Vec` argument to the ability function
- Register a function that reads from a thread-local target list

Use the thread-local approach to keep the script API clean:

In `game_module.rs`, add:
```rust
thread_local! {
    static AVAILABLE_TARGETS: RefCell<Vec<Combatant>> = RefCell::new(Vec::new());
}

pub fn set_available_targets(targets: Vec<Combatant>) {
    AVAILABLE_TARGETS.with(|t| *t.borrow_mut() = targets);
}
```

Register in module:
```rust
module.function(
    "targets_in_cone",
    |source: &Combatant, range: f32, arc: f32| -> Vec<Combatant> {
        let half_arc_cos = (arc.to_radians() / 2.0).cos();
        AVAILABLE_TARGETS.with(|targets| {
            targets.borrow().iter().filter(|t| {
                crate::combat::cone_hit_check(
                    glam::Vec2::new(source.pos_x, source.pos_z),
                    glam::Vec2::new(source.dir_x, source.dir_z),
                    glam::Vec2::new(t.pos_x, t.pos_z),
                    range,
                    half_arc_cos,
                )
            }).cloned().collect()
        })
    },
)?;

module.function(
    "targets_in_radius",
    |pos_x: f32, pos_z: f32, radius: f32| -> Vec<Combatant> {
        AVAILABLE_TARGETS.with(|targets| {
            targets.borrow().iter().filter(|t| {
                let dx = t.pos_x - pos_x;
                let dz = t.pos_z - pos_z;
                (dx * dx + dz * dz).sqrt() <= radius
            }).cloned().collect()
        })
    },
)?;
```

**Step 2: Add call_ability method to ScriptEngine**

```rust
pub fn call_ability(
    &self,
    function: &str,
    source: Combatant,
    targets: Vec<Combatant>,
    rng_roll: f32,
) -> Result<Vec<Command>, String> {
    set_rng_roll(rng_roll);
    game_module::set_available_targets(targets);

    let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
    vm.call([function], (source,))
        .map_err(|e| e.to_string())?;

    Ok(take_commands())
}
```

**Step 3: Write the melee attack script**

Create `client/assets/scripts/abilities/melee_attack.rune`:
```rune
use game::*;

pub fn on_ability_start(source) {
    animate(source, "attack");
    sound("swoosh", source.pos_x, source.pos_y, source.pos_z);

    let targets = targets_in_cone(source, source.attack_range, source.attack_arc);

    for target in targets {
        let hit = Hit {
            damage: source.attack_damage,
            knockback: source.knockback_force,
            is_crit: false,
        };

        // on_pre_hit hooks would be called here by the engine
        // (fire_hook is a Task 9 concern — for now, inline crit check)

        damage(target, hit.damage);
        knockback(target, hit.knockback);

        // on_hit hooks would be called here
    }
}
```

**Note:** `fire_hook` (running other scripts' hooks mid-script) is complex and deferred to Task 9. For now, the melee script just does the basic loop. The hook chaining will be layered on top.

**Step 4: Write test with mock targets, verify commands emitted**

**Step 5: Run tests, commit**

```bash
git add client/assets/scripts/abilities/melee_attack.rune core/src/scripting/
git commit -m "Implement melee attack ability as Rune script"
```

---

## Task 8: Write ground_pound ability script

**Files:**
- Create: `client/assets/scripts/abilities/ground_pound.rune`

**Step 1: Write the ground pound script**

Create `client/assets/scripts/abilities/ground_pound.rune`:
```rune
use game::*;

pub fn on_ability_start(source) {
    animate(source, "ground_pound");
    sound("ground_pound", source.pos_x, source.pos_y, source.pos_z);
    vfx("ground_pound_shockwave", source);

    let targets = targets_in_radius(source.pos_x, source.pos_z, 6.0);
    let base_damage = source.attack_damage * 4.0;

    for target in targets {
        let hit = Hit {
            damage: base_damage,
            knockback: 20.0,
            is_crit: false,
        };

        damage(target, hit.damage);
        knockback(target, hit.knockback);
    }

    screen_shake(1.5);
}
```

**Step 2: Write test, run, commit**

```bash
git add client/assets/scripts/abilities/ground_pound.rune core/src/scripting/
git commit -m "Implement ground pound ability as Rune script"
```

---

## Task 9: Implement hook chaining (fire_hook)

This is the key composability feature — an ability script can fire `on_pre_hit` which runs all attached behavior scripts' `on_pre_hit` functions.

**Files:**
- Create: `core/src/scripting/registry.rs`
- Modify: `core/src/scripting/mod.rs`
- Modify: `core/src/scripting/game_module.rs`

**Step 1: Create ScriptRegistry that tracks which scripts are attached to entities**

Create `core/src/scripting/registry.rs`:
```rust
use crate::scripting::ScriptEngine;
use std::collections::HashMap;

/// Stores compiled scripts by name.
pub struct ScriptRegistry {
    scripts: HashMap<String, ScriptEngine>,
}

impl ScriptRegistry {
    pub fn new() -> Self {
        Self {
            scripts: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: String, source: &str) -> Result<(), String> {
        let engine = ScriptEngine::compile(source)?;
        self.scripts.insert(name, engine);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ScriptEngine> {
        self.scripts.get(name)
    }
}

/// Which scripts are attached to an entity, for each hook.
pub struct EntityScripts {
    pub behaviors: Vec<String>, // script names, executed in order
}
```

**Step 2: Implement fire_hook**

`fire_hook` needs to: take a hook name, find all behavior scripts attached to the current entity, and call that hook on each one, passing the (potentially mutated) hit through.

This is the hardest part of the system. The approach:
1. `fire_hook` is registered as a Rune function
2. It reads entity behavior list from a thread-local
3. It calls each behavior's hook function, threading the `Hit` through
4. Each behavior can mutate the hit; next behavior sees the mutations

This requires the script registry to be accessible from within Rune execution. Use thread-local for the registry reference.

**Step 3: Write integration test**

Test that an ability script calling `fire_hook("on_pre_hit")` runs the crit behavior, which modifies the hit, and the ability then sees the modified damage.

**Step 4: Run tests, commit**

```bash
git add core/src/scripting/
git commit -m "Implement hook chaining (fire_hook) for composable behaviors"
```

---

## Task 10: Write zombie_ai enemy script

Port `server/src/enemy_ai.rs` AI decision logic to Rune.

**Files:**
- Create: `client/assets/scripts/enemies/zombie_ai.rune`
- Modify: `core/src/scripting/game_module.rs` (add `nearest_player` support)

**Step 1: Add nearest_player to game module**

Similar to `targets_in_cone`, use a thread-local for the player list:
```rust
thread_local! {
    static AVAILABLE_PLAYERS: RefCell<Vec<Combatant>> = RefCell::new(Vec::new());
}

pub fn set_available_players(players: Vec<Combatant>) {
    AVAILABLE_PLAYERS.with(|p| *p.borrow_mut() = players);
}
```

Register:
```rust
module.function(
    "nearest_player",
    |pos_x: f32, pos_z: f32| -> Option<Combatant> {
        AVAILABLE_PLAYERS.with(|players| {
            players.borrow().iter().min_by(|a, b| {
                let da = (a.pos_x - pos_x).powi(2) + (a.pos_z - pos_z).powi(2);
                let db = (b.pos_x - pos_x).powi(2) + (b.pos_z - pos_z).powi(2);
                da.partial_cmp(&db).unwrap()
            }).cloned()
        })
    },
)?;
```

**Step 2: Add `call_tick` method to ScriptEngine**

```rust
pub fn call_tick(
    &self,
    function: &str,
    entity: Combatant,
    players: Vec<Combatant>,
    dt: f32,
    rng_roll: f32,
) -> Result<Vec<Command>, String> {
    set_rng_roll(rng_roll);
    game_module::set_available_players(players);

    let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
    vm.call([function], (entity, dt))
        .map_err(|e| e.to_string())?;

    Ok(take_commands())
}
```

**Step 3: Write the zombie AI script**

Create `client/assets/scripts/enemies/zombie_ai.rune`:
```rune
use game::*;

pub fn on_tick(self_entity, dt) {
    let player = nearest_player(self_entity.pos_x, self_entity.pos_z);

    if player is None {
        set_behavior(self_entity, "idle");
        return;
    }

    let player = player.unwrap();
    let dist = distance_2d(self_entity, player);

    if dist <= self_entity.attack_range && self_entity.cooldown_ready {
        set_behavior(self_entity, "attack");
        damage(player, self_entity.attack_damage);
    } else if dist <= 15.0 {
        set_behavior(self_entity, "chase");
        move_toward(self_entity, player.pos_x, player.pos_z, self_entity.speed * dt);
    } else {
        set_behavior(self_entity, "idle");
    }
}
```

**Note:** The `Option` handling syntax in Rune may differ — check Rune docs for pattern matching on `Option`. May need `if let Some(player) = nearest_player(...) { ... }` instead.

**Step 4: Write test with mock player positions, verify chase/attack/idle decisions**

**Step 5: Run tests, commit**

```bash
git add client/assets/scripts/enemies/zombie_ai.rune core/src/scripting/
git commit -m "Implement zombie AI as Rune script"
```

---

## Task 11: Integrate Rune into the client's combat system

Wire the Rune scripts into the actual Bevy game. Replace the Rust `on_attack_hit` with Rune script execution.

**Files:**
- Create: `client/src/scripting.rs` (Bevy plugin for script loading + execution)
- Modify: `client/src/combat/attack.rs` (call Rune instead of `resolve_combat`)
- Modify: `client/Cargo.toml` (no new deps — `game-core` already has Rune)
- Modify: `client/src/main.rs` (add scripting plugin)

**Step 1: Create the client scripting plugin**

Create `client/src/scripting.rs`:
- `ScriptRegistryResource` — wraps `core::scripting::registry::ScriptRegistry` as a Bevy `Resource`
- Startup system that loads `.rune` files from `assets/scripts/` and compiles them
- `EntityScripts` component — list of behavior script names attached to an entity
- `ActiveAbility` component — which ability script this entity uses

**Step 2: Create client CommandExecutor**

A Bevy system that reads commands from the buffer and applies them:
- `Command::DealDamage` → send `DamageDealt` event
- `Command::SpawnVfx` → spawn particle entity (or send event for existing VFX systems)
- `Command::PlaySound` → send audio event
- `Command::Animate` → update animation state
- `Command::ScreenShake` / `Command::HitStop` → send `HitLanded` event with feedback
- `Command::ApplyKnockback` → insert `PendingKnockback` component

**Step 3: Modify attack.rs**

In `on_attack_hit`, instead of calling `resolve_combat()`:
1. Build `Combatant` from player's ECS components
2. Build target `Combatant` list from `Query<(Entity, &Transform, &Health), With<Enemy>>`
3. Call `script_engine.call_ability("on_ability_start", source, targets, rng_roll)`
4. Apply returned commands via the executor

**Step 4: Test in-game**

Run: `just`
Expected: Melee attacks work as before — damage numbers, knockback, crit effects — but driven by Rune scripts.

**Step 5: Commit**

```bash
git add client/src/scripting.rs client/src/combat/attack.rs client/src/main.rs
git commit -m "Integrate Rune scripting into client combat system"
```

---

## Task 12: Integrate Rune into the server's combat reducer

Wire the Rune scripts into SpacetimeDB server.

**Files:**
- Modify: `server/src/combat.rs` (call Rune instead of `resolve_combat`)
- Modify: `server/src/enemy_ai.rs` (call Rune for AI decisions)
- Modify: `server/src/lib.rs` (initialize script registry)

**Step 1: Create server-side script initialization**

In `server/src/lib.rs`, in the `init` reducer, compile all scripts from embedded strings:
```rust
use game_core::scripting::registry::ScriptRegistry;

thread_local! {
    static SCRIPTS: RefCell<ScriptRegistry> = RefCell::new({
        let mut reg = ScriptRegistry::new();
        reg.register("crit".into(), include_str!("../../core/gameplay/behaviors/crit.rune")).unwrap();
        reg.register("stacking".into(), include_str!("../../core/gameplay/behaviors/stacking.rune")).unwrap();
        reg.register("melee_attack".into(), include_str!("../../core/gameplay/abilities/melee_attack.rune")).unwrap();
        reg.register("ground_pound".into(), include_str!("../../core/gameplay/abilities/ground_pound.rune")).unwrap();
        reg.register("zombie_ai".into(), include_str!("../../core/gameplay/enemies/zombie_ai.rune")).unwrap();
        reg
    });
}
```

**Step 2: Create server CommandExecutor**

A function that takes `Command` list and applies them to SpacetimeDB tables:
- `Command::DealDamage` → update Enemy/Player health, insert `CombatEvent`
- `Command::ApplyKnockback` → insert `KnockbackImpulse`
- `Command::AddBuff` → insert/update `ActiveEffect`
- `Command::SetStat` → update Player/Enemy stats
- `Command::SetBehavior` → update Enemy behavior column
- `Command::MoveToward` → update Enemy position
- VFX/Sound/Animate commands → insert `CombatEvent` for clients to react to, or no-op

**Step 3: Modify attack_hit reducer**

Replace `resolve_combat()` call with Rune script execution. Build `Combatant` from Player table row, build targets from Enemy table rows, call script, apply commands.

**Step 4: Modify game_tick for AI**

Replace inline AI logic with Rune script execution per enemy. Call `zombie_ai.on_tick()` for each enemy, apply resulting commands.

**Step 5: Test end-to-end**

Run: `just` (starts SpacetimeDB + deploys + runs client)
Expected: Combat and enemy AI work as before, now driven by Rune on both sides.

**Step 6: Verify WASM compilation**

Run: `just check` (which includes `cargo check` for wasm32 target)
Expected: PASS

**Step 7: Commit**

```bash
git add server/src/
git commit -m "Integrate Rune scripting into server combat and enemy AI"
```

---

## Task 13: Remove old rules system

Now that Rune handles all game logic, remove the Rust rules system.

**Files:**
- Delete: `core/src/rules.rs`
- Delete: `core/src/presets/` (entire directory)
- Delete: `client/assets/presets/*.preset.ron`
- Modify: `core/src/lib.rs` (remove `pub mod rules; pub mod presets;`)
- Modify: `core/src/combat.rs` (remove `resolve_combat`, `resolve_attack`, keep geometry helpers like `cone_hit_check`, `knockback_displacement`, constants)
- Modify: `client/models/src/combat.rs` (remove Stats wrapper if replaced)
- Modify: `client/src/combat/attack.rs` (remove `OnPreHitRules` etc. component queries)
- Modify: `server/src/combat.rs` (remove `PLAYER_RULES` thread-local, old `resolve_combat` calls)

**Step 1: Remove old code**

Remove the files and references listed above. Keep:
- `core/src/combat.rs` geometry functions (`cone_hit_check`, `knockback_displacement`, `can_attack`)
- `core/src/combat.rs` constants (`defaults` module)
- `core/src/rng.rs` (still used by scripting)

**Step 2: Run tests**

Run: `cargo test`
Expected: PASS — all tests now use Rune scripts.

**Step 3: Run full check**

Run: `just check`
Expected: PASS

**Step 4: Commit**

```bash
git add -A
git commit -m "Remove old Rust rules system, replaced by Rune scripts"
```

---

## Task 14: Add hot-reload support for dev builds

**Files:**
- Modify: `client/src/scripting.rs`

**Step 1: Watch script files for changes**

Use Bevy's `AssetServer` file watcher (already enabled via the `default` feature's `file_watcher`). When a `.rune` file changes:
1. Recompile the changed script
2. Swap the bytecode in `ScriptRegistry`
3. Log the reload to console

**Step 2: Test hot reload**

Run: `just`
Edit `crit.rune` while game is running (e.g., change `crit_multiplier` multiplier to 10x).
Expected: Next attack uses the new multiplier without restarting.

**Step 3: Commit**

```bash
git add client/src/scripting.rs
git commit -m "Add hot-reload for Rune scripts in dev builds"
```

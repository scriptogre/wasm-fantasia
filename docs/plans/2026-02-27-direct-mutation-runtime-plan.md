# Direct Mutation Runtime Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the command buffer scripting model with direct mutation, where Rune scripts mutate game state immediately through semantic primitives and can read the results of their own writes.

**Architecture:** Scripts call semantic primitives (`apply_damage`, `heal`, etc.) that mutate Combatant structs in-place and log intent. Presentation functions (`vfx`, `sound`) append to a side-effect log on client and are no-ops on server. After script returns, the engine reads intents/effects to update real game state.

**Tech Stack:** Rune 0.14.1, Bevy 0.18, SpacetimeDB 1.12.0, Rust

---

### Task 1: Fix broken include_str! paths from previous move

The `.rune` files were moved from `client/assets/scripts/` to `core/gameplay/` but the `include_str!()` paths in both client and server scripting modules were never updated.

**Files:**
- Modify: `client/src/scripting.rs` (lines 34, 41, 48, 55, 62, 70 — include_str! paths; lines 162, 206 — hot-reload PathBuf)
- Modify: `server/src/scripting.rs` (lines 10, 15, 20, 25 — include_str! paths)

**Step 1: Update client include_str! paths**

In `client/src/scripting.rs`, change all 6 `include_str!` paths:
- `"../assets/scripts/behaviors/crit.rune"` → `"../../core/gameplay/behaviors/crit.rune"`
- `"../assets/scripts/behaviors/stacking.rune"` → `"../../core/gameplay/behaviors/stacking.rune"`
- `"../assets/scripts/behaviors/feedback.rune"` → `"../../core/gameplay/behaviors/feedback.rune"`
- `"../assets/scripts/abilities/melee_attack.rune"` → `"../../core/gameplay/abilities/melee_attack.rune"`
- `"../assets/scripts/abilities/ground_pound.rune"` → `"../../core/gameplay/abilities/ground_pound.rune"`
- `"../assets/scripts/enemies/zombie_ai.rune"` → `"../../core/gameplay/enemies/zombie_ai.rune"`

Change 2 hot-reload `PathBuf` references:
- `"client/assets/scripts"` → `"core/gameplay"` (lines 162 and 206)

**Step 2: Update server include_str! paths**

In `server/src/scripting.rs`, change all 4 `include_str!` paths:
- `"../../client/assets/scripts/behaviors/crit.rune"` → `"../../core/gameplay/behaviors/crit.rune"`
- `"../../client/assets/scripts/behaviors/stacking.rune"` → `"../../core/gameplay/behaviors/stacking.rune"`
- `"../../client/assets/scripts/abilities/melee_attack.rune"` → `"../../core/gameplay/abilities/melee_attack.rune"`
- `"../../client/assets/scripts/abilities/ground_pound.rune"` → `"../../core/gameplay/abilities/ground_pound.rune"`

**Step 3: Remove empty old directory**

```bash
rm -rf client/assets/scripts/
```

**Step 4: Verify compilation**

```bash
cargo check -p game-client && cargo check -p game-server && cargo test -p game-core
```
Expected: All pass.

**Step 5: Commit**

```bash
git add -A && git commit -m "Fix include_str paths after script move to core/gameplay"
```

---

### Task 2: Rename module from `scripting` to `runtime`

Rename the Rust module and update all imports across the workspace.

**Files:**
- Rename: `core/src/scripting/` → `core/src/runtime/`
- Modify: `core/src/lib.rs` (line 3: `pub mod scripting` → `pub mod runtime`)
- Modify: `client/src/scripting.rs` (line 12: `game_core::scripting::` → `game_core::runtime::`)
- Modify: `client/src/combat/attack.rs` (lines 9-10: `game_core::scripting::` → `game_core::runtime::`)
- Modify: `server/src/scripting.rs` (line 3: `game_core::scripting::` → `game_core::runtime::`)
- Modify: `server/src/combat.rs` (line 2: `game_core::scripting::` → `game_core::runtime::`)

**Step 1: Rename the directory**

```bash
mv core/src/scripting core/src/runtime
```

**Step 2: Update core/src/lib.rs**

Change line 3 from `pub mod scripting;` to `pub mod runtime;`.

**Step 3: Update all imports**

Find and replace `game_core::scripting` with `game_core::runtime` in:
- `client/src/scripting.rs`
- `client/src/combat/attack.rs`
- `server/src/scripting.rs`
- `server/src/combat.rs`

**Step 4: Verify compilation**

```bash
cargo check -p game-core && cargo check -p game-client && cargo check -p game-server && cargo test -p game-core
```
Expected: All pass.

**Step 5: Commit**

```bash
git add -A && git commit -m "Rename scripting module to runtime"
```

---

### Task 3: Create Intent and Effect enums, replace Command/CommandBuffer

Replace the command buffer pattern with Intent (state changes) and Effect (presentation) enums.

**Files:**
- Delete: `core/src/runtime/commands.rs`
- Modify: `core/src/runtime/api.rs` (currently `game_module.rs` — will be renamed in next task)
- Modify: `core/src/runtime/mod.rs` (remove Command/CommandBuffer re-exports)

**Step 1: Write the failing test**

Add to end of `core/src/runtime/api.rs` (currently `game_module.rs`), inside a `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_log_collects_intents() {
        clear_logs();
        push_intent(Intent::DamageDealt { target_id: 1, amount: 50.0 });
        push_intent(Intent::Healed { target_id: 2, amount: 25.0 });
        let intents = take_intents();
        assert_eq!(intents.len(), 2);
        assert!(matches!(intents[0], Intent::DamageDealt { target_id: 1, amount } if (amount - 50.0).abs() < f32::EPSILON));
    }

    #[test]
    fn effect_log_collects_effects() {
        clear_logs();
        push_effect(Effect::Vfx { name: "slash".into(), target_id: 1 });
        push_effect(Effect::Sound { name: "hit".into(), target_id: 1 });
        let effects = take_effects();
        assert_eq!(effects.len(), 2);
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p game-core intent_log_collects
```
Expected: FAIL — `Intent`, `push_intent`, `take_intents`, `clear_logs` don't exist.

**Step 3: Implement Intent and Effect enums with thread-local logs**

At the top of `game_module.rs` (before the existing thread-locals), add:

```rust
/// An intent records that a state-changing action occurred during script execution.
/// The engine reads these after the script returns to update real game state.
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

/// A presentation effect requested by a script. Client processes these after
/// script execution; server ignores them (the functions are no-ops).
#[derive(Debug, Clone)]
pub enum Effect {
    Vfx { name: String, target_id: u64 },
    Sound { name: String, target_id: u64 },
    ScreenShake { intensity: f32 },
    HitStop { duration: f32 },
    Animate { entity_id: u64, animation: String },
}

thread_local! {
    static INTENT_LOG: RefCell<Vec<Intent>> = RefCell::new(Vec::new());
    static EFFECT_LOG: RefCell<Vec<Effect>> = RefCell::new(Vec::new());
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
```

**Step 4: Run test to verify it passes**

```bash
cargo test -p game-core intent_log_collects && cargo test -p game-core effect_log_collects
```
Expected: PASS.

**Step 5: Remove commands.rs**

Delete `core/src/runtime/commands.rs` and remove `pub mod commands;` from `core/src/runtime/mod.rs`. Remove the `pub use commands::{Command, CommandBuffer};` line. Don't worry about compilation errors in other files yet — those will be fixed in later tasks.

**Step 6: Commit**

```bash
git add -A && git commit -m "Add Intent and Effect enums, remove command buffer"
```

---

### Task 4: Rename game_module.rs to api.rs, change Rune crate name to gameplay

**Files:**
- Rename: `core/src/runtime/game_module.rs` → `core/src/runtime/api.rs`
- Modify: `core/src/runtime/mod.rs` (change `pub mod game_module` → `pub mod api`, update imports)

**Step 1: Rename the file**

```bash
mv core/src/runtime/game_module.rs core/src/runtime/api.rs
```

**Step 2: Update mod.rs**

In `core/src/runtime/mod.rs`:
- Replace `pub mod game_module;` with `pub mod api;`
- Replace all `game_module::` references with `api::`
- Update the re-exports to use `api::` prefix

**Step 3: Change Rune module crate name**

In `core/src/runtime/api.rs`, change the `build_game_module` function:
- Rename function to `build_gameplay_module`
- Change `Module::with_crate("game")` to `Module::with_crate("gameplay")`

Update `core/src/runtime/mod.rs` to call `build_gameplay_module` instead of `build_game_module`.

**Step 4: Verify compilation**

```bash
cargo test -p game-core game_module_builds
```
Expected: PASS (the test name may need updating if it references the old name).

**Step 5: Commit**

```bash
git add -A && git commit -m "Rename game_module to api, change Rune crate name to gameplay"
```

---

### Task 5: Implement direct mutation state primitives

Replace the old buffer-pushing functions with direct mutation + intent logging.

**Files:**
- Modify: `core/src/runtime/api.rs`

**Step 1: Write failing tests for apply_damage**

Add to the test module in `api.rs`:

```rust
#[test]
fn apply_damage_mutates_and_logs() {
    clear_logs();
    let mut target = Combatant {
        id: 1, health: 100.0, max_health: 100.0,
        pos_x: 0.0, pos_y: 0.0, pos_z: 0.0, dir_x: 1.0, dir_z: 0.0,
        attack_damage: 10.0, crit_chance: 0.0, crit_multiplier: 1.5,
        knockback_force: 5.0, attack_range: 2.0, attack_arc: 90.0,
        attack_speed: 1.0, fury_stacks: 0, attack_speed_bonus: 0.0,
        cooldown_ready: true, speed: 5.0,
    };
    let dealt = apply_damage_impl(&mut target, 30.0);
    assert!((dealt - 30.0).abs() < f32::EPSILON);
    assert!((target.health - 70.0).abs() < f32::EPSILON);
    let intents = take_intents();
    assert_eq!(intents.len(), 1);
    assert!(matches!(intents[0], Intent::DamageDealt { target_id: 1, amount } if (amount - 30.0).abs() < f32::EPSILON));
}

#[test]
fn apply_damage_clamps_to_zero() {
    clear_logs();
    let mut target = Combatant {
        id: 1, health: 20.0, max_health: 100.0,
        pos_x: 0.0, pos_y: 0.0, pos_z: 0.0, dir_x: 1.0, dir_z: 0.0,
        attack_damage: 10.0, crit_chance: 0.0, crit_multiplier: 1.5,
        knockback_force: 5.0, attack_range: 2.0, attack_arc: 90.0,
        attack_speed: 1.0, fury_stacks: 0, attack_speed_bonus: 0.0,
        cooldown_ready: true, speed: 5.0,
    };
    let dealt = apply_damage_impl(&mut target, 50.0);
    assert!((dealt - 20.0).abs() < f32::EPSILON, "should only deal 20 (remaining health)");
    assert!(target.health <= 0.0);
}

#[test]
fn heal_mutates_and_logs() {
    clear_logs();
    let mut target = Combatant {
        id: 1, health: 60.0, max_health: 100.0,
        pos_x: 0.0, pos_y: 0.0, pos_z: 0.0, dir_x: 1.0, dir_z: 0.0,
        attack_damage: 10.0, crit_chance: 0.0, crit_multiplier: 1.5,
        knockback_force: 5.0, attack_range: 2.0, attack_arc: 90.0,
        attack_speed: 1.0, fury_stacks: 0, attack_speed_bonus: 0.0,
        cooldown_ready: true, speed: 5.0,
    };
    let healed = heal_impl(&mut target, 50.0);
    assert!((healed - 40.0).abs() < f32::EPSILON, "should only heal 40 (to max)");
    assert!((target.health - 100.0).abs() < f32::EPSILON);
    let intents = take_intents();
    assert!(matches!(intents[0], Intent::Healed { target_id: 1, amount } if (amount - 40.0).abs() < f32::EPSILON));
}
```

**Step 2: Run tests to verify they fail**

```bash
cargo test -p game-core apply_damage_mutates
```
Expected: FAIL — `apply_damage_impl` doesn't exist.

**Step 3: Implement direct mutation functions**

Replace the old `damage`, `heal`, `knockback`, etc. functions in `api.rs` with new implementations that mutate Combatant and log intents. The `_impl` suffix versions take `&mut Combatant` for testing; the Rune-exposed versions use Rune's `Mut<Combatant>`.

```rust
/// Reduce target's health by amount. Returns actual damage dealt (clamped to remaining health).
/// Mutates target.health immediately and logs a DamageDealt intent.
pub(crate) fn apply_damage_impl(target: &mut Combatant, amount: f32) -> f32 {
    let actual = amount.min(target.health).max(0.0);
    target.health -= actual;
    push_intent(Intent::DamageDealt { target_id: target.id, amount: actual });
    actual
}

/// Rune-exposed version: `apply_damage(target, amount) -> f32`
fn apply_damage(mut target: rune::runtime::Mut<Combatant>, amount: f32) -> f32 {
    apply_damage_impl(&mut target, amount)
}

pub(crate) fn heal_impl(target: &mut Combatant, amount: f32) -> f32 {
    let actual = amount.min(target.max_health - target.health).max(0.0);
    target.health += actual;
    push_intent(Intent::Healed { target_id: target.id, amount: actual });
    actual
}

fn heal(mut target: rune::runtime::Mut<Combatant>, amount: f32) -> f32 {
    heal_impl(&mut target, amount)
}

fn apply_knockback(target: &Combatant, force: f32) {
    push_intent(Intent::KnockbackApplied { target_id: target.id, force });
}

fn add_buff(target: &Combatant, name: &str, duration: f32) {
    push_intent(Intent::BuffAdded { target_id: target.id, name: name.to_string(), duration });
}

fn remove_buff(target: &Combatant, name: &str) {
    push_intent(Intent::BuffRemoved { target_id: target.id, name: name.to_string() });
}

fn set_stat(mut entity: rune::runtime::Mut<Combatant>, stat: &str, value: f32) {
    // Direct mutation of known fields
    match stat {
        "fury_stacks" => entity.fury_stacks = value as i64,
        "attack_speed_bonus" => entity.attack_speed_bonus = value,
        "health" => entity.health = value,
        "max_health" => entity.max_health = value,
        "attack_damage" => entity.attack_damage = value,
        "attack_speed" => entity.attack_speed = value,
        "speed" => entity.speed = value,
        _ => {}
    }
    push_intent(Intent::StatSet { entity_id: entity.id, stat: stat.to_string(), value });
}

fn kill(mut target: rune::runtime::Mut<Combatant>) {
    target.health = 0.0;
    push_intent(Intent::Killed { target_id: target.id });
}

fn set_behavior(entity: &Combatant, behavior: &str) {
    push_intent(Intent::BehaviorSet { entity_id: entity.id, behavior: behavior.to_string() });
}

fn move_toward(entity: &Combatant, target_x: f32, target_z: f32, speed: f32) {
    push_intent(Intent::MovedToward { entity_id: entity.id, target_x, target_z, speed });
}
```

**Important Rune note:** Functions that mutate their argument must take `rune::runtime::Mut<Combatant>` instead of `&mut Combatant`. Rune's `Mut<T>` is like a `RefMut` — it allows the script to see the mutation on subsequent reads. Register these with `.build()` as before.

Update `build_gameplay_module()` to register the new function names:
- `damage` → `apply_damage`
- `heal` → `heal` (unchanged)
- `knockback` → `apply_knockback`
- `buff` → `add_buff`
- `remove_buff` → `remove_buff` (unchanged)
- Add `kill`

Keep presentation functions as-is but change them to push effects:

```rust
fn vfx(name: &str, target: &Combatant) {
    push_effect(Effect::Vfx { name: name.to_string(), target_id: target.id });
}

fn sound(name: &str, target: &Combatant) {
    push_effect(Effect::Sound { name: name.to_string(), target_id: target.id });
}

fn screen_shake(intensity: f32) {
    push_effect(Effect::ScreenShake { intensity });
}

fn hit_stop(duration: f32) {
    push_effect(Effect::HitStop { duration });
}

fn animate(entity: &Combatant, animation: &str) {
    push_effect(Effect::Animate { entity_id: entity.id, animation: animation.to_string() });
}
```

**Note on `sound` signature change:** The old `sound(name, x, y, z)` takes explicit coordinates. The new `sound(name, target)` takes a Combatant reference. This is simpler and consistent with other functions. Scripts that need sound at a specific position can pass the relevant entity.

**Step 4: Run tests**

```bash
cargo test -p game-core apply_damage && cargo test -p game-core heal_mutates
```
Expected: PASS.

**Step 5: Remove old COMMAND_BUFFER thread-local**

Remove the `COMMAND_BUFFER` thread-local and `push_command`/`take_commands` functions from `api.rs`. The old `Command`-based functions are fully replaced.

**Step 6: Commit**

```bash
git add -A && git commit -m "Implement direct mutation state primitives with intent logging"
```

---

### Task 6: Update ScriptEngine to return intents and effects

Change `ScriptEngine` methods to return `(Vec<Intent>, Vec<Effect>)` instead of `Vec<Command>`.

**Files:**
- Modify: `core/src/runtime/mod.rs`

**Step 1: Update method signatures**

Change:
- `call_ability` returns `Result<(Vec<Intent>, Vec<Effect>), ...>` instead of `Result<Vec<Command>, ...>`
- `call_ability_with_behaviors` returns `Result<(Vec<Intent>, Vec<Effect>), ...>`
- `call_tick` returns `Result<(Vec<Intent>, Vec<Effect>), ...>`
- `call_hit_hook` returns `Result<(Hit, Vec<Intent>, Vec<Effect>), ...>`

Implementation changes:
```rust
pub fn call_ability(
    &self,
    function: &str,
    source: Combatant,
    targets: Vec<Combatant>,
    rng_roll: f32,
) -> Result<(Vec<Intent>, Vec<Effect>), rune::support::Error> {
    clear_logs();
    set_rng_roll(rng_roll);
    api::set_available_targets(targets);

    let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
    vm.call([function], (source,))?;

    Ok((take_intents(), take_effects()))
}
```

Apply the same pattern to `call_ability_with_behaviors`, `call_tick`, and `call_hit_hook`.

**Step 2: Update re-exports in mod.rs**

Replace:
```rust
pub use api::{clear_logs, take_intents, take_effects, Intent, Effect};
```

Remove the old `Command`/`CommandBuffer` re-exports and `take_commands`.

**Step 3: Update existing tests**

All tests in `mod.rs` that assert on `Vec<Command>` need updating to assert on `Vec<Intent>` and `Vec<Effect>` instead. For example:

```rust
// Old:
assert!(matches!(cmds[0], Command::DealDamage { target_id: 2, amount } if ...));

// New:
let (intents, effects) = engine.call_ability(...)?;
assert!(matches!(intents[0], Intent::DamageDealt { target_id: 2, amount } if ...));
```

This is a large mechanical refactor of ~27 tests. Key patterns:
- `Command::DealDamage` → `Intent::DamageDealt`
- `Command::Heal` → `Intent::Healed`
- `Command::ApplyKnockback` → `Intent::KnockbackApplied`
- `Command::SetStat` → `Intent::StatSet`
- `Command::AddBuff` → `Intent::BuffAdded`
- `Command::SetBehavior` → `Intent::BehaviorSet`
- `Command::MoveToward` → `Intent::MovedToward`
- `Command::SpawnVfx` → `Effect::Vfx`
- `Command::PlaySound` → `Effect::Sound`
- `Command::Animate` → `Effect::Animate`
- `Command::ScreenShake` → `Effect::ScreenShake`
- `Command::HitStop` → `Effect::HitStop`

Tests that previously checked `cmds.len()` now need to check `intents.len() + effects.len()` or check each separately.

**Step 4: Run tests**

```bash
cargo test -p game-core
```
Expected: All 27+ tests pass.

**Step 5: Commit**

```bash
git add -A && git commit -m "Update ScriptEngine to return intents and effects"
```

---

### Task 7: Update .rune scripts for new API

Change all scripts from `use game::*` to `use gameplay::*` and update function names.

**Files:**
- Modify: `core/gameplay/abilities/melee_attack.rune`
- Modify: `core/gameplay/abilities/ground_pound.rune`
- Modify: `core/gameplay/behaviors/crit.rune`
- Modify: `core/gameplay/behaviors/stacking.rune`
- Modify: `core/gameplay/behaviors/feedback.rune`
- Modify: `core/gameplay/enemies/zombie_ai.rune`

**Changes per file:**

All files: `use game::*;` → `use gameplay::*;`

`melee_attack.rune`:
- `damage(target, hit.damage)` → `apply_damage(target, hit.damage)`
- `knockback(target, hit.knockback)` → `apply_knockback(target, hit.knockback)`

`ground_pound.rune`:
- `damage(target, hit.damage)` → `apply_damage(target, hit.damage)`
- `knockback(target, hit.knockback)` → `apply_knockback(target, hit.knockback)`

`stacking.rune`:
- `buff(source, "fury", 2.5)` → `add_buff(source, "fury", 2.5)`

`feedback.rune`:
- `sound("impact", target.pos_x, target.pos_y, target.pos_z)` → `sound("impact", target)` (new signature takes Combatant)

`zombie_ai.rune`:
- `damage(player, self_entity.attack_damage)` → `apply_damage(player, self_entity.attack_damage)`

`crit.rune`:
- Only `use game::*;` → `use gameplay::*;` (no function renames needed)

**Step 1: Update all scripts**

Apply the changes above to each file.

**Step 2: Update inline test scripts in mod.rs**

All test constants (`CRIT_SCRIPT`, `STACKING_SCRIPT`, `FEEDBACK_SCRIPT`, `MELEE_ATTACK_SCRIPT`, `GROUND_POUND_SCRIPT`, `MELEE_ATTACK_WITH_HOOKS`, `GROUND_POUND_WITH_HOOKS`, `ZOMBIE_AI_SCRIPT`) need the same changes: `use game::*` → `use gameplay::*` and function renames.

**Step 3: Run tests**

```bash
cargo test -p game-core
```
Expected: All pass.

**Step 4: Commit**

```bash
git add -A && git commit -m "Update Rune scripts for gameplay:: namespace and new function names"
```

---

### Task 8: Update client executor to use intents and effects

Change the client-side command processing to read `Intent` and `Effect` instead of `Command`.

**Files:**
- Modify: `client/src/combat/attack.rs`
- Modify: `client/src/scripting.rs` (update imports)

**Step 1: Update imports**

In `client/src/combat/attack.rs`:
```rust
// Old:
use game_core::scripting::Command as ScriptCommand;
use game_core::scripting::types::Combatant as ScriptCombatant;

// New:
use game_core::runtime::{Intent, Effect};
use game_core::runtime::types::Combatant as ScriptCombatant;
```

In `client/src/scripting.rs`:
```rust
// Old:
use game_core::scripting::registry::ScriptRegistry;

// New:
use game_core::runtime::registry::ScriptRegistry;
```

**Step 2: Rewrite process_script_commands**

The function currently takes `&[ScriptCommand]` and switches on Command variants. Rewrite to take `(&[Intent], &[Effect])`:

```rust
fn process_script_results(
    intents: &[Intent],
    effects: &[Effect],
    attacker_entity: Entity,
    origin_pos: Vec3,
    forward: Vec3,
    targets: &Query<(Entity, &Transform, &Health), With<Enemy>>,
    stats: &mut Option<Mut<'_, Stats>>,
    bevy_commands: &mut Commands,
) -> bool {
    let mut any_crit = false;
    let mut target_hits: HashMap<u64, (f32, f32)> = HashMap::new();

    for intent in intents {
        match intent {
            Intent::DamageDealt { target_id, amount } => {
                let entry = target_hits.entry(*target_id).or_insert((0.0, 0.0));
                entry.0 += amount;
            }
            Intent::KnockbackApplied { target_id, force } => {
                let entry = target_hits.entry(*target_id).or_insert((0.0, 0.0));
                entry.1 = *force;
            }
            Intent::StatSet { stat, value, .. } => {
                if let Some(s) = stats {
                    s.set(stat_from_name(stat), *value);
                }
            }
            _ => {}
        }
    }

    // Check effects for crit indicators
    for effect in effects {
        if let Effect::Vfx { name, target_id } = effect {
            if name == "crit_particles" {
                if target_hits.contains_key(target_id) {
                    any_crit = true;
                }
            }
        }
    }

    // ... rest of knockback/DamageDealt event firing (same logic as before)

    any_crit
}
```

**Step 3: Update callers**

In `on_attack_hit` and `on_ground_pound_hit`, change from:
```rust
let script_cmds = ability_engine.call_ability_with_behaviors(...)?;
process_script_commands(&script_cmds, ...);
```
To:
```rust
let (intents, effects) = ability_engine.call_ability_with_behaviors(...)?;
process_script_results(&intents, &effects, ...);
```

**Step 4: Verify compilation**

```bash
cargo check -p game-client
```
Expected: Compiles.

**Step 5: Commit**

```bash
git add -A && git commit -m "Update client executor to use intents and effects"
```

---

### Task 9: Update server executor to use intents and effects

Change the server-side command processing to read `Intent` instead of `Command`. Presentation effects are already no-ops on the server (never reach this code), so only intents matter.

**Files:**
- Modify: `server/src/combat.rs`
- Modify: `server/src/scripting.rs`

**Step 1: Update server/src/scripting.rs**

Change imports and function return types:
```rust
use game_core::runtime::{Combatant, Intent, Effect, ScriptRegistry};
```

Change `run_melee_attack` and `run_ground_pound` to return `(Vec<Intent>, Vec<Effect>)`:
```rust
pub fn run_melee_attack(
    source: Combatant,
    targets: Vec<Combatant>,
    rng_roll: f32,
) -> (Vec<Intent>, Vec<Effect>) {
    SCRIPTS.with(|reg| {
        let engine = reg.get("melee_attack").expect("melee_attack script must be registered");
        let behaviors = vec!["crit".into(), "stacking".into()];
        engine
            .call_ability_with_behaviors(
                "on_ability_start",
                source, targets, rng_roll,
                reg.clone(), behaviors,
            )
            .expect("melee_attack script execution failed")
    })
}
```

**Step 2: Update server/src/combat.rs**

Rewrite `process_combat_commands` to use `Intent` instead of `Command`:

```rust
fn process_combat_intents(
    ctx: &spacetimedb::ReducerContext,
    intents: &[Intent],
    attacker: &Player,
    world_id: u32,
    enemy_pos_index: &std::collections::HashMap<u64, (f32, f32, f32)>,
    fwd: &glam::Vec2,
    now: i64,
    hit_any: &mut bool,
    new_stacks: &mut f32,
    new_speed_bonus: &mut f32,
    buff_applied: &mut bool,
) {
    let mut damage_by_target: std::collections::HashMap<u64, f32> = std::collections::HashMap::new();
    let mut knockback_by_target: std::collections::HashMap<u64, f32> = std::collections::HashMap::new();

    for intent in intents {
        match intent {
            Intent::DamageDealt { target_id, amount } => {
                *damage_by_target.entry(*target_id).or_insert(0.0) += amount;
                *hit_any = true;
            }
            Intent::KnockbackApplied { target_id, force } => {
                *knockback_by_target.entry(*target_id).or_insert(0.0) += force;
            }
            Intent::StatSet { stat, value, .. } => {
                if stat == "fury_stacks" { *new_stacks = *value; }
                else if stat == "attack_speed_bonus" { *new_speed_bonus = *value; }
            }
            Intent::BuffAdded { .. } => { *buff_applied = true; }
            _ => {}
        }
    }

    // ... rest of damage application logic (same as before, minus is_crit from SpawnVfx)
}
```

**Note:** The old code detected crits by looking for `SpawnVfx { name: "crit_particles" }`. With the new model, crit detection should come from the `Hit.is_crit` field that the script already computes. For now, we can look for a crit-related effect in the effects log, or better: add an `is_crit: bool` field to `Intent::DamageDealt`. This is a design improvement to make in this task.

Update `Intent::DamageDealt` to:
```rust
DamageDealt { target_id: u64, amount: f32, is_crit: bool },
```

And update `apply_damage_impl` to accept the crit flag from the Hit context. This may require threading through the `is_crit` state, or having the crit VFX logic derive from the intent directly.

**Step 3: Update callers**

In `attack_hit` and `aoe_hit`, change from:
```rust
let commands = scripting::run_melee_attack(source, targets, rng_roll);
process_combat_commands(ctx, &commands, ...);
```
To:
```rust
let (intents, _effects) = scripting::run_melee_attack(source, targets, rng_roll);
process_combat_intents(ctx, &intents, ...);
```

The `_effects` are ignored on the server.

**Step 4: Verify compilation**

```bash
cargo check -p game-server
```
Expected: Compiles.

**Step 5: Commit**

```bash
git add -A && git commit -m "Update server executor to use intents"
```

---

### Task 10: Final verification and cleanup

**Step 1: Run all tests**

```bash
cargo test -p game-core
```
Expected: All tests pass.

**Step 2: Check all crates compile**

```bash
cargo check -p game-core && cargo check -p game-client && cargo check -p game-server
```
Expected: All pass.

**Step 3: Check WASM compilation**

```bash
cargo check -p game-server --target wasm32-unknown-unknown
```
Expected: Pass.

**Step 4: Run clippy**

```bash
cargo clippy --workspace -- -D warnings
```
Expected: No warnings.

**Step 5: Verify file structure is clean**

```
core/src/runtime/
  mod.rs       — Engine
  api.rs       — Intent, Effect, all functions
  registry.rs  — ScriptRegistry
  types.rs     — Combatant, Hit

core/gameplay/
  abilities/melee_attack.rune
  abilities/ground_pound.rune
  behaviors/crit.rune
  behaviors/stacking.rune
  behaviors/feedback.rune
  enemies/zombie_ai.rune
```

No leftover `commands.rs`, no `client/assets/scripts/`, no `game_module.rs`.

**Step 6: Commit any final cleanup**

```bash
git add -A && git commit -m "Final cleanup: verify all crates compile and tests pass"
```

# Rune-Driven Enemy AI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace hardcoded `enemy_ai_decision()` with a Rune batch call per enemy type, enabling per-type AI scripts with hot-reload.

**Architecture:** Rust pre-computes spatial data (nearest player distance, cooldown state), groups enemies by type, calls Rune once per type with a batch of inputs, and applies the returned behavior decisions. The game_tick loop, movement, separation, physics, and DB writes stay in Rust.

**Tech Stack:** Rune scripting engine (already integrated), game-core runtime module, SpacetimeDB server

---

### Task 1: Add AiInput type for Rune

**Files:**
- Modify: `core/src/runtime/types.rs` (append after Hit struct, ~line 55)
- Modify: `core/src/runtime/api.rs` (register type in build_gameplay_module)

**Step 1: Add AiInput struct to types.rs**

Append after the `Hit` struct:

```rust
#[derive(Any, Debug, Clone)]
#[rune(constructor)]
pub struct AiInput {
    #[rune(get)]
    pub dist: f64,
    #[rune(get)]
    pub range: f64,
    #[rune(get)]
    pub cooldown: bool,
}
```

Note: Rune uses f64 for all numbers. The `#[rune(constructor)]` enables `AiInput { dist, range, cooldown }` syntax in scripts.

**Step 2: Export AiInput from mod.rs**

In `core/src/runtime/mod.rs`, add `AiInput` to the `pub use types::{Combatant, Hit};` line:

```rust
pub use types::{AiInput, Combatant, Hit};
```

**Step 3: Register AiInput in the gameplay module**

In `core/src/runtime/api.rs`, inside `build_gameplay_module()`, add after the existing type registrations:

```rust
module.ty::<super::types::AiInput>()?;
```

**Step 4: Verify it compiles**

Run: `cargo check -p game-core`

**Step 5: Commit**

```
feat: add AiInput type for Rune enemy AI scripts
```

---

### Task 2: Add call_batch_decide to ScriptEngine

**Files:**
- Modify: `core/src/runtime/mod.rs` (add method after call_hit_hook, ~line 157)

**Step 1: Write test for batch decide**

Add to the existing `mod tests` block in `core/src/runtime/mod.rs`:

```rust
#[test]
fn batch_decide_returns_decisions() {
    let script = r#"
        use gameplay::*;
        pub fn decide_batch(inputs) {
            let results = [];
            for input in inputs {
                if input.dist <= input.range && input.cooldown {
                    results.push(2);
                } else if input.dist <= 15.0 {
                    results.push(1);
                } else {
                    results.push(0);
                }
            }
            results
        }
    "#;
    let engine = ScriptEngine::new(script).expect("should compile");
    let inputs = vec![
        AiInput { dist: 1.5, range: 2.0, cooldown: true },   // attack (2)
        AiInput { dist: 10.0, range: 2.0, cooldown: false },  // chase (1)
        AiInput { dist: 20.0, range: 2.0, cooldown: false },  // idle (0)
    ];
    let results = engine.call_batch_decide("decide_batch", inputs).expect("should succeed");
    assert_eq!(results, vec![2, 1, 0]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p game-core batch_decide`
Expected: FAIL — `call_batch_decide` doesn't exist yet.

**Step 3: Implement call_batch_decide**

Add to `impl ScriptEngine` in `core/src/runtime/mod.rs`, after `call_hit_hook`:

```rust
/// Call a batch AI decision function: `fn decide_batch(inputs) -> Vec<int>`.
///
/// Passes a Rune Vec of `AiInput` objects and expects a Rune Vec of
/// integer behavior IDs back (0=Idle, 1=Chase, 2=Attack).
pub fn call_batch_decide(
    &self,
    function: &str,
    inputs: Vec<AiInput>,
) -> Result<Vec<u8>, rune::support::Error> {
    let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
    let output = vm.call([function], (inputs,))?;
    let results: Vec<i64> = rune::from_value(output)?;
    Ok(results.into_iter().map(|v| v as u8).collect())
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p game-core batch_decide`
Expected: PASS

**Step 5: Commit**

```
feat: add call_batch_decide for Rune enemy AI batch calls
```

---

### Task 3: Add enemy_type_name mapping

**Files:**
- Modify: `core/src/combat.rs` (append after enemy_types module, ~line 131)

**Step 1: Add enemy_type_name function**

After the `enemy_types` module:

```rust
/// Map enemy_type ID to script name prefix.
/// Convention: type "zombie" loads "zombie_ai" from the registry.
pub fn enemy_type_name(t: u8) -> &'static str {
    match t {
        enemy_types::BASIC => "zombie",
        _ => "zombie",
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo check -p game-core`

**Step 3: Commit**

```
feat: add enemy_type_name mapping for script lookup
```

---

### Task 4: Rewrite zombie_ai.rune to batch API

**Files:**
- Modify: `core/runes/enemies/zombie_ai.rune`

**Step 1: Rewrite the script**

```rune
use gameplay::*;

pub fn decide_batch(inputs) {
    let results = [];
    for input in inputs {
        if input.dist <= input.range && input.cooldown {
            results.push(2);
        } else if input.dist <= 15.0 {
            results.push(1);
        } else {
            results.push(0);
        }
    }
    results
}
```

**Step 2: Test compilation via existing test infrastructure**

Run: `cargo test -p game-core`
Expected: PASS (the existing compile tests should still work, and the new script compiles)

**Step 3: Commit**

```
feat: rewrite zombie_ai.rune to batch decide API
```

---

### Task 5: Register zombie_ai in server ScriptRegistry

**Files:**
- Modify: `server/src/scripting.rs` (add registration in SCRIPTS thread_local, ~line 27)

**Step 1: Add zombie_ai registration**

In the `SCRIPTS` thread_local block, before `Arc::new(reg)`:

```rust
reg.register(
    "zombie_ai".into(),
    include_str!("../../core/runes/enemies/zombie_ai.rune"),
)
.expect("zombie_ai script should compile");
```

**Step 2: Add run_enemy_ai function**

After `run_ground_pound`, add:

```rust
/// Run batch AI decisions for a group of enemies via Rune.
/// Returns a Vec of u8 behavior IDs (0=Idle, 1=Chase, 2=Attack),
/// one per input. Falls back to hardcoded decisions on script error.
pub fn run_enemy_ai_batch(
    enemy_type: u8,
    inputs: Vec<game_core::runtime::AiInput>,
) -> Vec<u8> {
    let script_name = format!("{}_ai", game_core::combat::enemy_type_name(enemy_type));
    let fallback = || {
        inputs
            .iter()
            .map(|input| {
                game_core::combat::enemy_ai_decision(input.dist as f32, input.cooldown).as_u8()
            })
            .collect()
    };

    SCRIPTS.with(|reg| {
        let Some(engine) = reg.get(&script_name) else {
            spacetimedb::log::warn!("No AI script '{script_name}', using fallback");
            return fallback();
        };
        match engine.call_batch_decide("decide_batch", inputs) {
            Ok(results) => results,
            Err(e) => {
                spacetimedb::log::warn!("AI script '{script_name}' failed: {e}, using fallback");
                fallback()
            }
        }
    })
}
```

**Step 3: Verify it compiles**

Run: `cargo check -p game-server`

**Step 4: Commit**

```
feat: register zombie_ai and add run_enemy_ai_batch in server scripting
```

---

### Task 6: Wire Rune batch call into game_tick

**Files:**
- Modify: `server/src/enemy_ai.rs` (~line 302-387, the per-enemy loop)

This is the core integration. The current code computes `nearest_dist` and `attack_cooldown_ready` per enemy, then calls `enemy_ai_decision()` inline. We need to:

1. Group enemies by type (currently all BASIC, but future-proof)
2. Build AiInput vec per type
3. Call `run_enemy_ai_batch()` once per type
4. Use the returned decisions

**Step 1: Add import at top of enemy_ai.rs**

```rust
use game_core::runtime::AiInput;
```

**Step 2: Restructure the per-enemy loop**

Replace the current code block (lines ~318-387) that computes decisions and movement. The structure becomes:

```rust
// Phase 1: Pre-compute spatial data for each enemy
struct EnemyPrecomputed {
    nearest_dist: f32,
    nearest_pos: (f32, f32),
    cooldown_ready: bool,
    has_knockback: bool,
    is_airborne: bool,
}

let mut precomputed: Vec<EnemyPrecomputed> = Vec::with_capacity(enemies.len());
for (idx, enemy) in enemies.iter().enumerate() {
    // ... existing nearest-player and cooldown computation (lines 323-341) ...
    precomputed.push(EnemyPrecomputed {
        nearest_dist,
        nearest_pos,
        cooldown_ready: attack_cooldown_ready,
        has_knockback,
        is_airborne,
    });
}

// Phase 2: Build AiInput batch and call Rune
// (All enemies are currently the same type, but this groups by type for future use)
let ai_inputs: Vec<AiInput> = precomputed
    .iter()
    .zip(enemies.iter())
    .map(|(pre, enemy)| AiInput {
        dist: pre.nearest_dist as f64,
        range: enemy.attack_range as f64,
        cooldown: pre.cooldown_ready,
    })
    .collect();

let decisions = crate::scripting::run_enemy_ai_batch(
    enemies[0].enemy_type,
    ai_inputs,
);

// Phase 3: Apply decisions and compute movement (same as before)
let mut updates: Vec<EnemyUpdate> = Vec::with_capacity(enemies.len());
let mut airborne_indices: Vec<usize> = Vec::with_capacity(64);

for (idx, enemy) in enemies.iter().enumerate() {
    let pre = &precomputed[idx];
    let decision = combat::EnemyBehaviorKind::from_u8(decisions[idx]);

    // ... rest of movement computation unchanged (chase velocity, separation, airborne check) ...
}
```

The key change is: `enemy_ai_decision(nearest_dist, attack_cooldown_ready)` on line 338 becomes `combat::EnemyBehaviorKind::from_u8(decisions[idx])`.

**Step 3: Verify it compiles**

Run: `cargo check -p game-server`

**Step 4: Run the game and test**

Start SpacetimeDB and deploy: `just spacetimedb`
Run the client: `cargo run`
Spawn enemies (E key) and verify:
- Enemies chase when near player
- Enemies attack when in range with cooldown ready
- Enemies idle when far away
- Behavior is identical to before (same logic, just in Rune)

**Step 5: Commit**

```
feat: wire Rune batch AI calls into server game_tick

Replaces hardcoded enemy_ai_decision() with per-type Rune script
calls. Falls back to Rust if script is missing or fails.
```

---

### Task 7: Verify hot-reload works (client only)

**Files:**
- No changes needed — client/src/scripting.rs already registers zombie_ai and hot-reloads the enemies/ directory.

**Step 1: Test hot-reload**

1. Run the game in dev mode: `cargo run`
2. Spawn enemies
3. Edit `core/runes/enemies/zombie_ai.rune` — change the chase distance from 15.0 to 5.0
4. Wait 1 second for hot-reload ("Rune scripts hot-reloaded" in console)
5. Verify enemies now only chase within 5m instead of 15m

Note: Hot-reload only affects the client's copy of the scripts. The server embeds scripts at compile time. For server changes, redeploy the module.

**Step 2: Commit (if any fixups needed)**

---

## Summary

| Task | What | Files |
|------|-------|-------|
| 1 | AiInput Rune type | types.rs, api.rs, mod.rs |
| 2 | call_batch_decide method + test | mod.rs |
| 3 | enemy_type_name mapping | combat.rs |
| 4 | Rewrite zombie_ai.rune | zombie_ai.rune |
| 5 | Server script registration + fallback | scripting.rs |
| 6 | Wire into game_tick | enemy_ai.rs |
| 7 | Verify hot-reload | manual test |

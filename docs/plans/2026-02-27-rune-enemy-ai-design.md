# Rune-Driven Enemy AI

## Problem

Enemy AI decisions are hardcoded in `enemy_ai_decision()` (core/src/combat.rs). Adding new enemy types with different behavior requires modifying Rust code and recompiling. We want designers to define AI behavior per enemy type via .rune scripts, with hot-reload support on the client.

## Design

Replace `enemy_ai_decision()` with a Rune batch call per enemy type. The Rust `game_tick` loop is unchanged — it pre-computes spatial data and applies results. Only the decision function moves to Rune.

### Data flow per tick

```
Rust: spatial grid -> nearest_dist per enemy -> group by enemy_type
         |
Rune: zombie_ai.decide_batch([{dist, range, cooldown}, ...]) -> [1, 1, 2, 0, ...]
         |
Rust: apply behaviors -> movement -> DB write
```

### What stays in Rust

- The game_tick iteration loop (10K enemies)
- Spatial queries (nearest player, spatial grid, separation)
- Movement math (chase velocity, knockback physics)
- DB writes (batch update changed enemies)

### What moves to Rune

The behavior decision: given pre-computed inputs, pick idle/chase/attack.

### Script API

Each enemy type gets `core/runes/enemies/{type}_ai.rune` with one function:

```rune
// zombie_ai.rune
pub fn decide_batch(inputs) {
    let results = [];
    for input in inputs {
        if input.dist <= input.range && input.cooldown {
            results.push(2); // attack
        } else if input.dist <= 15.0 {
            results.push(1); // chase
        } else {
            results.push(0); // idle
        }
    }
    results
}
```

The `input` object exposes: `dist` (f32), `range` (f32), `cooldown` (bool). These are pre-computed by Rust. No spatial queries from Rune.

### Type name mapping

`enemy_types::BASIC` (u8 = 0) maps to `"zombie"` via `enemy_type_name()` in core/src/combat.rs. The ScriptRegistry looks up `"{name}_ai"` to find the compiled script.

Convention: `core/runes/enemies/zombie_ai.rune` for enemy type "zombie".

### Performance

Batch call: one Rune VM invocation per enemy type per tick. The Rune bytecode loop iterates all enemies of that type. VM setup cost is amortized across 10K enemies instead of paid 10K times. Expected overhead: <5ms for 10K enemies.

### Fallback

If the Rune call fails (script error, missing function, wrong return type), fall back to `enemy_ai_decision()` for that batch. Log a warning.

### Changes

| File | Change |
|------|--------|
| `core/src/runtime/mod.rs` | Add `call_batch_decide()` to ScriptEngine |
| `core/src/runtime/api.rs` | Add `AiInput` type exposed to Rune |
| `core/src/combat.rs` | Add `enemy_type_name()` mapping |
| `server/src/enemy_ai.rs` | Replace `enemy_ai_decision()` with Rune batch call |
| `server/src/scripting.rs` | Register enemy AI scripts |
| `core/runes/enemies/zombie_ai.rune` | Rewrite to `decide_batch` API |
| `client/src/scripting.rs` | Include enemy scripts in hot-reload |

### What doesn't change

- game_tick loop structure
- EnemyBehaviorKind enum and u8 encoding
- Movement logic (chase, separation, knockback)
- Client animation pipeline
- Ability/behavior hook system

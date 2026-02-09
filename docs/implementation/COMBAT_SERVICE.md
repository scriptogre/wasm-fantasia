# Combat Service Layer

Status: **Plan — not yet implemented.**

## Goal

Move the entire attack resolution loop into `resolve_combat()` in `shared/src/combat.rs`. Both client and server call it — zero duplicated game logic. Simultaneously rename events to follow the Intent → Mutation → Feedback convention and consolidate them into a single manifest file.

## Event Renames

Past tense for outcomes, noun form for requests. The tense tells you the role.

| Current | New | Role | File |
|---------|-----|------|------|
| `AttackHit` | `AttackIntent` | Intent — attack hit frame reached | `combat/events.rs` |
| `DamageEvent` | `DamageDealt` | Mutation — health changes | `combat/events.rs` |
| `HitEvent` | `HitLanded` | Feedback — VFX/sound/shake | `combat/events.rs` |
| `DeathEvent` | `Died` | Cross-domain mutation | `combat/events.rs` |
| `CritHitEvent` | `CritHit` | Sub-feedback (rules) | `rules/triggers.rs` |
| `CritKillEvent` | `CritKill` | Sub-feedback (rules) | `rules/triggers.rs` |

Attack chain: `AttackIntent` → `DamageDealt` → `HitLanded`
Death chain: `DamageDealt` → `Died` (cross-domain — any source can kill)
Crit chain: `HitLanded` → `CritHit`, `Died` → `CritKill`

## New File: `client/src/combat/events.rs`

All combat events live here. Module-level comment documents the chains. Each event has a doc comment with `[`backtick`]` cross-references to related events (Rust doc warns on broken links — free sync checking).

`CritHit`/`CritKill` stay in `rules/triggers.rs` — they're rules-system events, not core combat. Cross-referenced via doc links.

## The Shared Function

`resolve_combat()` encapsulates the full attack-against-targets flow:

1. Per-target deterministic RNG roll
2. `resolve_attack()` per target (damage calc, crit, pre_hit rules) — already exists
3. Cone hit check (with crit range bonus — `CRIT_RANGE_BONUS = 1.3`, moved from client)
4. Health calculation + death check
5. On-hit / on-crit-hit / on-kill rule dispatch
6. Returns structured results — callers just apply to their storage

### Types (in `shared/src/combat.rs`)

```rust
pub const CRIT_RANGE_BONUS: f32 = 1.3;

pub struct HitTarget {
    pub id: u64,
    pub pos: Vec2,
    pub health: f32,
}

pub struct HitResult {
    pub target_id: u64,
    pub damage: f32,
    pub is_crit: bool,
    pub knockback: f32,
    pub push: f32,
    pub launch: f32,
    pub new_health: f32,
    pub died: bool,
    pub feedback: HitFeedback,
}

pub struct CombatInput<'a> {
    pub origin: Vec2,
    pub forward: Vec2,
    pub base_range: f32,
    pub half_arc_cos: f32,
    pub attacker_stats: &'a Stats,
    pub rules: &'a EntityRules,
    pub rng_seed: u64,
    pub targets: &'a [HitTarget],
}

pub struct CombatOutput {
    pub hits: Vec<HitResult>,
    pub attacker_stats: Stats,  // after all on_hit/on_crit_hit/on_kill rules
    pub hit_any: bool,
}
```

## Client Caller (`client/src/combat/attack.rs`)

`on_attack_hit` (renamed observer for `AttackIntent`) becomes:

1. Build `HitTarget` list from ECS query (entity→id mapping via `Entity::to_bits()`)
2. Build `CombatInput` from attacker stats, transform, rules components
3. Call `resolve_combat()`
4. Write back `CombatOutput.attacker_stats` to ECS `Stats` component
5. For each `HitResult`: fire `DamageDealt` with damage, force, is_crit, feedback
6. Fixes per-target RNG bug (client currently uses one roll for all targets)

**Client always runs `resolve_combat()` locally in both modes.** VFX must be instant. The `on_damage` observer (for `DamageDealt`) gates health mutation on whether the entity is server-owned, not on whether multiplayer is active. This preserves the current behavior where attack animation + damage numbers + screen shake play immediately, and server health sync overwrites local state.

## Server Caller (`server/src/lib.rs`)

`attack_hit` reducer becomes:

1. Build `HitTarget` list from NpcEnemy table query
2. Build `CombatInput` from Player row stats
3. Call `resolve_combat()`
4. For each `HitResult`: insert `CombatEvent`, update/delete NpcEnemy health
5. Write back `CombatOutput.attacker_stats` to Player row (stacks, attack_speed)

Replaces ~60 lines of manual cone loop + rule dispatch.

## Double-Application Prevention

`resolve_combat()` now executes on_hit, on_crit_hit, and on_kill rules internally. The client observers in `rules/triggers.rs` that currently execute these rules must be stripped of rule execution:

- `on_hit_observer`: Remove `execute_rules()` call. Keep the observer only for firing `CritHit` if `is_crit` (read from `DamageDealt` event data, not from re-executing rules).
- `on_crit_hit_observer`: Remove `execute_rules()` call. Observer becomes a no-op or is removed entirely.
- `on_kill_observer`: Remove `execute_rules()` call. Keep for firing `CritKill` if the kill was a crit.

Actually, simpler: since `resolve_combat()` returns `is_crit` per hit and `died` per hit, the client's `on_attack_hit` observer can fire `CritHit` / `CritKill` directly from the results. The intermediate observers in triggers.rs for rule execution become unnecessary. They can be stripped to stubs or removed, with `CritHit`/`CritKill` fired directly from the attack resolution observer.

## Knockback (Minimal, Server)

Add `vel_x`/`vel_z` to `NpcEnemy` table. Server applies knockback force from `HitResult` as velocity impulse. `game_tick` steps NPC positions: `pos += vel * dt; vel *= friction`. Clients interpolate positions from sync.

Intentionally simple. Will be replaced by avian3d-core when MOVEMENT.md work begins.

## Files to Modify

| File | Change |
|------|--------|
| `shared/src/combat.rs` | Add `CRIT_RANGE_BONUS`, `HitTarget`, `HitResult`, `CombatInput`, `CombatOutput`, `resolve_combat()` |
| `client/src/combat/events.rs` | **New file.** Consolidate `AttackIntent`, `DamageDealt`, `HitLanded`, `Died` with doc cross-references |
| `client/src/combat/components.rs` | Remove `AttackHit`, `DamageEvent`, `DeathEvent` (moved to events.rs). Keep `Health`, `AttackState`, markers |
| `client/src/combat/mod.rs` | Add `mod events`, update `pub use` |
| `client/src/combat/attack.rs` | Rewrite `on_attack_hit` to call `resolve_combat()`, fire `DamageDealt` + `CritHit`/`CritKill` from results |
| `client/src/combat/damage.rs` | Rename `DamageEvent`→`DamageDealt`, `DeathEvent`→`Died` in observer signatures. Gate health on server-owned markers (already does this). |
| `client/src/combat/hit_feedback.rs` | Rename `HitEvent`→`HitLanded` in all observer signatures |
| `client/src/combat/sound.rs` | Rename `HitEvent`→`HitLanded` in observer signature |
| `client/src/rules/triggers.rs` | Strip rule execution from `on_hit_observer`, `on_crit_hit_observer`, `on_kill_observer`, `on_crit_kill_observer`. These become stubs or are removed (CritHit/CritKill fired from attack.rs instead). Rename event imports. |
| `server/src/lib.rs` | Rewrite `attack_hit` to call `resolve_combat()`. Add `vel_x`/`vel_z` to NpcEnemy. Add NPC velocity stepping to `game_tick`. |
| `client/src/networking/combat.rs` | Interpolate NPC positions from server sync instead of snapping |

## Verification

1. `cargo check` on all three crates
2. `just` — native build, attack enemies, verify damage/crit/stacking/death/knockback
3. `just` (with two clients) — multiplayer, verify knockback syncs across clients

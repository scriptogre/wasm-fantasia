# Separation of Concerns — Refactoring Plan

Status: **Approved plan. Work through in order.**

Reference: `CLAUDE.md` (Architecture Conventions), `docs/architecture/PATTERNS.md` (research)

## Problem

Networking code directly constructs domain entity bundles, domain code imports networking types behind `cfg(feature)`, and some event flows skip the Request→Outcome pipeline. These violations make it impossible to change domain internals without touching networking, and vice versa.

## Refactors (Priority Order)

### 1. Introduce `ServerAuthoritative` marker

**Why**: Eliminates all `cfg(feature = "multiplayer")` from combat code. Domain modules check a single marker instead of querying networking-specific components.

**Files**:
- `combat/components.rs` — Add `ServerAuthoritative` marker component
- `combat/damage.rs` — Replace `cfg(feature)` + `RemotePlayer`/`ServerEnemy` queries with `Query<(), With<ServerAuthoritative>>`
- `networking/combat.rs` — Add `ServerAuthoritative` when spawning server enemies
- `networking/player.rs` — Add `ServerAuthoritative` when spawning remote players

**Result**: `combat/damage.rs` has zero networking imports and zero feature flags.

---

### 2. Extract enemy archetype into `combat/enemy.rs`

**Why**: `networking/combat.rs` currently builds the full enemy entity (Health, Stats, Combatant, Collider, Mesh, RigidBody, Mass, LockedAxes). When you add a component to enemies, you'd need to update networking too.

**Approach**: Define a spawn event and a builder function in combat/enemy.rs:

```rust
// combat/enemy.rs
#[derive(Event)]
pub struct SpawnServerEnemy {
    pub server_id: u64,
    pub position: Vec3,
    pub health: f32,
    pub max_health: f32,
}

fn on_spawn_server_enemy(on: On<SpawnServerEnemy>, mut commands: Commands, ...) {
    let e = on.event();
    commands.spawn((
        // Full enemy archetype — owned by combat module
        ServerAuthoritative,
        ServerEnemyId(e.server_id),  // thin ID, no networking types
        Name::new(format!("ServerEnemy_{}", e.server_id)),
        Health::new(e.max_health), Enemy, Combatant,
        Stats::new().with(Stat::MaxHealth, e.max_health).with(Stat::Health, e.health),
        Collider::capsule(0.5, 1.0), RigidBody::Dynamic, LockedAxes::ROTATION_LOCKED, Mass(500.0),
        // mesh + material
    ));
}
```

**Networking becomes**:
```rust
// networking/combat.rs — sync_npc_enemies
// For new enemies: just fire the event
commands.trigger(SpawnServerEnemy { server_id: enemy.id, position, health, max_health });
// For existing enemies: update position/health on entities found by ServerEnemyId
// For removed enemies: despawn by ServerEnemyId
```

**Files**:
- `combat/enemy.rs` — Add `SpawnServerEnemy` event, `ServerEnemyId` component, observer
- `combat/components.rs` — Move `ServerEnemy` → `ServerEnemyId` (thin ID, no networking dep)
- `networking/combat.rs` — Remove bundle construction, fire events instead. Remove `Collider`, `RigidBody`, `Mass`, `LockedAxes`, `Health`, `Stats`, `Enemy`, `Combatant` imports
- `combat/mod.rs` — Export `SpawnServerEnemy`, `ServerEnemyId`

---

### 3. Extract remote player combat setup into `combat/` or `player/`

**Why**: Same problem as #2. `networking/player.rs` spawns remote players with `Health`, `Combatant`, `Stats`, `Collider` inline.

**Approach**: Similar event pattern. Networking fires `SpawnRemotePlayer { identity, position, rotation, health, max_health }`. A player/ or combat/ observer attaches combat components.

Alternative (simpler): Use a `Required` component or a setup observer that watches for `Added<RemotePlayer>` and attaches combat components. This avoids a new event type.

```rust
// combat/components.rs or player/remote.rs
fn setup_remote_combatant(
    new_remotes: Query<(Entity, &RemotePlayerData), Added<RemotePlayerData>>,
    mut commands: Commands,
) {
    for (entity, data) in new_remotes.iter() {
        commands.entity(entity).insert((
            Health::new(data.max_health),
            Combatant,
            ServerAuthoritative,
            Stats::new()
                .with(Stat::MaxHealth, data.max_health)
                .with(Stat::Health, data.health),
            Collider::capsule(0.35, 1.3),
        ));
    }
}
```

**Files**:
- `networking/player.rs` — Remove combat component imports, spawn with only networking-relevant components (RemotePlayer, InterpolatedPosition, Transform, mesh)
- `combat/components.rs` — Add `setup_remote_combatant` system watching `Added<RemotePlayer>`

---

### 4. Decouple `combat/enemy.rs` from networking reducer

**Why**: `spawn_enemy_in_front` directly calls `crate::networking::combat::server_spawn_enemies()`. Combat imports networking.

**Approach**: Fire a domain event. Networking observes it.

```rust
// combat/enemy.rs
#[derive(Event)]
pub struct SpawnEnemyRequest { pub position: Vec3, pub forward: Vec3 }

fn spawn_enemy_in_front(...) {
    // Always fire the request — let the right handler pick it up
    commands.trigger(SpawnEnemyRequest { position: pos, forward: forward.as_vec3() });
}

// networking/combat.rs
fn on_spawn_enemy_request(on: On<SpawnEnemyRequest>, conn: Res<SpacetimeDbConnection>) {
    let e = on.event();
    conn.conn.reducers.spawn_enemies(e.position.x, e.position.y, e.position.z, e.forward.x, e.forward.z);
}

// combat/enemy.rs — existing local spawn becomes the fallback observer
// Only runs when SpacetimeDbConnection is absent
fn on_spawn_enemy_local(on: On<SpawnEnemyRequest>, conn: Option<Res<SpacetimeDbConnection>>, ...) {
    if conn.is_some() { return; } // server handles it
    // ... spawn locally
}
```

**Files**:
- `combat/enemy.rs` — Replace direct reducer call with `SpawnEnemyRequest` event + local fallback observer
- `networking/combat.rs` — Add observer for `SpawnEnemyRequest`
- `combat/events.rs` — Add `SpawnEnemyRequest`

**Result**: `combat/enemy.rs` has zero networking imports.

---

### 5. Decouple `networking/combat.rs` health sync from direct mutation

**Why**: `sync_remote_health` and `sync_local_health` directly write to `Health.current` and `Stats`, bypassing any potential validation or event chain.

**Approach**: This one is lower priority and arguably acceptable — health sync IS authoritative state coming from the server. The mutation is intentional. But if we want consistency, introduce a `HealthSync` event that the health system processes.

For now: **defer**. Direct mutation of server-authoritative state is the correct pattern (Overwatch does the same). The marker from step 1 makes the intent clear.

---

### 6. Clean up remaining minor violations

**Low priority**, address opportunistically:

- `game/combat_debug.rs` reading SpacetimeDB tables directly — acceptable for debug tooling
- `screens/gameplay.rs` checking `SpacetimeDbConnection` — acceptable for UI state
- `player/animation.rs` reading `AttackState` — acceptable cross-domain read
- `camera/assist.rs` reading `LockedTarget` — acceptable cross-domain read

These are read-only queries across module boundaries. They don't create coupling problems because they don't construct entities or mutate foreign state.

## Execution Order

Do these sequentially — each builds on the previous:

1. **ServerAuthoritative marker** (15 min) — Unblocks everything else
2. **Enemy archetype extraction** (30 min) — Biggest SoC win
3. **Remote player combat setup** (20 min) — Same pattern as #2
4. **Enemy spawn event decoupling** (15 min) — Removes last networking import from combat/
5. Health sync — defer
6. Minor violations — defer

After steps 1-4: `combat/` has zero imports from `networking/`. The dependency arrow is strictly one-way: `networking → combat`, never the reverse.

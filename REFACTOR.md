# Separation of Concerns Refactoring

## Context

The client has game logic leaking across module boundaries. `combat/damage.rs` imports networking types behind `cfg(feature)`. `combat/enemy.rs` calls networking reducers directly. `networking/` has 6+ polling systems that each scan SpacetimeDB tables and build full domain entity bundles.

Goal: networking is a single translation layer. It receives server changes via callbacks and emits domain events. Domain modules observe these events and own their entity archetypes. `shared/` defines the protocol contract.

`GameMode`, SP/MP title buttons, and conditional networking connection are already in place.

## Step 1: `shared/src/protocol.rs` — Message Contract

Plain structs (no Bevy, no SpacetimeDB) defining the data that flows between client and backend.

```rust
pub struct EnemyState { pub id: u64, pub position: [f32; 3], pub health: f32, pub max_health: f32 }
pub struct PlayerState { pub id: u64, pub position: [f32; 3], pub rotation: f32, pub health: f32, pub max_health: f32, pub name: String }
pub struct HealthUpdate { pub id: u64, pub health: f32, pub max_health: f32 }
pub struct CombatHitReport { pub source_id: u64, pub target_id: u64, pub damage: f32, pub is_crit: bool }
```

**Files**: `shared/src/protocol.rs` (new), `shared/src/lib.rs` (add `pub mod protocol`)

## Step 2: Centralize Networking — Callbacks + Message Queue

Replace 6+ polling systems with callback-driven processing:

### 2a. Define `ServerMessage` enum and message queue

```rust
// networking/mod.rs
enum ServerMessage {
    PlayerJoined(Player),       // on_insert
    PlayerUpdated(Player),      // on_update
    PlayerLeft(Identity),       // on_delete
    EnemySpawned(NpcEnemy),     // on_insert
    EnemyUpdated(NpcEnemy),     // on_update
    EnemyRemoved(u64),          // on_delete
    CombatEvent(CombatEvent),   // on_insert
}

#[derive(Resource)]
struct ServerMessageQueue(Arc<Mutex<Vec<ServerMessage>>>);
```

### 2b. Register callbacks at connection time

When SpacetimeDB connects, register `on_insert`/`on_update`/`on_delete` callbacks on player, npc_enemy, and combat_event tables. Each callback pushes a `ServerMessage` to the shared queue.

### 2c. Single processing system

One system drains the queue each frame and fires domain events:

```rust
fn process_server_messages(
    queue: Res<ServerMessageQueue>,
    mut commands: Commands,
) {
    let messages: Vec<_> = queue.0.lock().unwrap().drain(..).collect();
    for msg in messages {
        match msg {
            ServerMessage::EnemySpawned(e) => {
                commands.trigger(SpawnEnemy { id: e.id, position: Vec3::new(e.x, e.y, e.z), ... });
            }
            ServerMessage::PlayerJoined(p) => {
                commands.trigger(SpawnPlayer { identity: p.identity, ... });
            }
            ServerMessage::PlayerUpdated(p) => {
                // Update InterpolatedPosition, health, animation state
            }
            // etc.
        }
    }
}
```

### 2d. Domain events (Bevy events in combat/events.rs and player/)

```rust
// combat/events.rs — new events
SpawnEnemy { id: u64, position: Vec3, health: f32, max_health: f32 }
UpdateEnemy { id: u64, position: Vec3, health: f32 }
RemoveEnemy { id: u64 }
RemoteCombatHit { source_id: u64, target_id: u64, damage: f32, is_crit: bool }
```

### What this replaces

| Old (polling) | New (callback → event) |
|---------------|----------------------|
| `sync_npc_enemies` | `ServerMessage::EnemySpawned/Updated/Removed` → `SpawnEnemy`/`UpdateEnemy`/`RemoveEnemy` |
| `spawn_remote_players` | `ServerMessage::PlayerJoined` → `SpawnPlayer` |
| `update_remote_players` | `ServerMessage::PlayerUpdated` → position/anim update |
| `sync_remote_health` | `ServerMessage::PlayerUpdated` → health update |
| `sync_local_health` | `ServerMessage::PlayerUpdated` (our identity) → health update |
| `process_remote_combat_events` | `ServerMessage::CombatEvent` → `RemoteCombatHit` |
| `handle_remote_death` | `ServerMessage::PlayerUpdated` (health ≤ 0) → hide entity |
| `despawn_remote_players` | `ServerMessage::PlayerLeft` → despawn |

**Files**: `networking/mod.rs`, `networking/combat.rs` (mostly deleted/simplified), `networking/player.rs` (mostly deleted/simplified), `combat/events.rs`

### What stays in networking

- `send_local_position` — outbound position sync (stays as-is, it's client→server)
- `send_attack_to_server` — outbound attack relay (stays)
- `process_outbound_lag` — lag simulation (stays)
- `connect_to_spacetimedb` — connection setup (modified to register callbacks)
- `measure_ping` — ping tracking (stays)

## Step 3: Domain Observers Handle Spawning

`combat/enemy.rs` observes `SpawnEnemy` and builds the full enemy archetype:

```rust
fn on_spawn_enemy(on: On<SpawnEnemy>, mut commands: Commands, mut meshes: ..., mut materials: ...) {
    let e = on.event();
    commands.spawn((
        EnemyId(e.id),
        Health::new(e.max_health), Enemy, Combatant,
        Stats::new()..., Collider, RigidBody, LockedAxes, Mass,
        mesh, material,
    ));
}
```

`EnemyId(u64)` is a thin component in `combat/components.rs`. Networking queries it for updates/removal without importing domain bundles.

For `UpdateEnemy` / `RemoveEnemy` — handled by combat observers that query `EnemyId`.

Similarly, `SpawnPlayer` would be handled by a player/ observer that builds the remote player entity with all components.

**Files**: `combat/events.rs`, `combat/components.rs`, `combat/enemy.rs`

## Step 4: Split `on_damage` — Prediction vs Backend

`combat/damage.rs:on_damage` does VFX feedback AND health mutation, using `cfg(feature)` + networking imports.

Split into two observers on `DamageDealt`:

- **`on_damage_feedback`** — always runs. Fires `HitLanded` for VFX/sound/shake. Applies knockback. Never touches Health.
- **`on_damage_apply`** — calls `health.take_damage()`, fires `Died`. Checks `GameMode`:

```rust
fn on_damage_apply(on: On<DamageDealt>, mode: Res<GameMode>, ...) {
    if *mode != GameMode::Singleplayer { return; }
    // health.take_damage(), fire Died
}
```

Same for `on_death` — despawn only in SP. In MP, entity lifecycle is handled by the server message queue (EnemyRemoved, PlayerLeft).

**Files**: `combat/damage.rs`

**Result**: Zero networking imports, zero `cfg(feature)` in `combat/damage.rs`.

## Step 5: `SpawnEnemyRequest` Event — Decouple Enemy Input

`combat/enemy.rs` calls `crate::networking::combat::server_spawn_enemies()` directly.

- Add `SpawnEnemyRequest { position: Vec3, forward: Vec3 }` event in `combat/events.rs`
- `combat/enemy.rs` on E key: fire `SpawnEnemyRequest`
- `combat/enemy.rs` observer: handles local spawn when `GameMode::Singleplayer`
- `networking/`: observer calls reducer when MP

**Files**: `combat/events.rs`, `combat/enemy.rs`, `networking/combat.rs`

**Result**: Zero networking imports in `combat/enemy.rs`.

## Verification

1. `just check` — clippy, fmt, machete, web compilation pass
2. `just` — SP: spawn enemies with E, hit them, VFX plays, health drains, they die
3. `just mp` — MP: enemies visible on both clients, remote attacks show VFX, health syncs
4. `grep -r "networking" client/src/combat/` — returns zero results
5. `grep -r "cfg.*multiplayer" client/src/combat/` — returns zero results

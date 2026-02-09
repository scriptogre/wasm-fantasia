# Server-Driven Reconciliation Refactor

## Problem

The client has 10+ bespoke networking systems (spawn_remote_players, sync_npc_enemies, sync_remote_health, handle_remote_death, etc.) that each poll SpacetimeDB tables with their own change detection and build domain-specific entity bundles. `combat/` imports networking types behind `cfg(feature)`. Adding a new synced entity type requires new polling systems, new events, new observers.

## Goal

One generic reconciler. The SpacetimeDB client cache is a local mirror of server state. Each frame, the reconciler diffs the cache against the ECS and patches what's different. No callbacks, no message queues, no per-entity-type sync systems. Like HTMX's morph — server describes the world, client renders it.

## Shared Types

`WorldEntity` and `CombatStats` are `#[spacetimedb::type]` structs embedded in every table that represents a game-world entity. The compiler guarantees all tables share the same fields.

```rust
// shared/src/schema.rs

#[spacetimedb::type]
pub struct WorldEntity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,
    pub health: f32,
    pub max_health: f32,
}

#[spacetimedb::type]
pub struct CombatStats {
    pub attack_damage: f32,
    pub crit_chance: f32,
    pub crit_multiplier: f32,
    pub attack_range: f32,
    pub attack_arc: f32,
    pub knockback_force: f32,
    pub attack_speed: f32,
    pub stacks: f32,
    pub stack_decay: f32,
    pub last_hit_time: i64,
    pub last_attack_time: i64,
    pub animation_state: String,
    pub attack_sequence: u32,
    pub attack_animation: String,
}
```

## Server Tables

Separate tables for type safety and scoped queries. Each embeds `WorldEntity` + `CombatStats`.

```rust
// server/src/lib.rs

#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: Identity,
    pub name: Option<String>,
    pub online: bool,
    pub last_update: i64,
    pub entity: WorldEntity,
    pub combat: CombatStats,
}

#[spacetimedb::table(name = enemy, public)]
pub struct Enemy {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub enemy_type: EnemyKind,
    pub entity: WorldEntity,
    pub combat: CombatStats,
}

#[spacetimedb::table(name = combat_event, public)]
pub struct CombatEvent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub attacker: Identity,
    pub target_player: Option<Identity>,
    pub target_enemy_id: Option<u64>,
    pub damage: f32,
    pub is_crit: bool,
    pub attacker_x: f32,
    pub attacker_z: f32,
    pub timestamp: i64,
}
```

Adding a field to `WorldEntity` or `CombatStats` updates all tables at once. Adding a new entity table (e.g. destructible crates with just `entity: WorldEntity`) requires one new `.chain()` call in the reconciler.

## Client: ServerId

A component that links an ECS entity to a server table row. The only per-type enum in the system.

```rust
#[derive(Component, Clone, Hash, Eq, PartialEq)]
pub enum ServerId {
    Player(Identity),
    Enemy(u64),
    CombatEvent(u64),
}
```

## The Reconciler

One function. Every table is another `.chain()`. `WorldEntity` and `CombatStats` are stored as components directly — no intermediate proxy types.

The local player already exists (spawned by the player module). The reconciler patches its health from the server row and skips spawning a duplicate. Remote entities are spawned, patched, or despawned to match server state.

Combat events are just another table. Spawning an entity with `CombatEventData` triggers an `On<Add, CombatEventData>` observer for VFX. The server deletes stale rows, the reconciler despawns the entity next frame.

```rust
fn reconcile(
    conn: Res<SpacetimeDbConnection>,
    mut remote_entities: Query<(Entity, &ServerId, &mut WorldEntity, &mut CombatStats)>,
    mut local_health: Query<&mut Health, With<Player>>,
    mut combat_events: Query<(Entity, &ServerId), With<CombatEventData>>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    let my_id = conn.conn.try_identity();
    let mut seen = HashSet::new();

    // ── Collect all entity tables into one flat list ───
    let rows: Vec<(ServerId, WorldEntity, CombatStats, &str, f32)> =
        conn.db.player().iter()
            .map(|p| (ServerId::Player(p.identity), p.entity, p.combat, "player.glb", 0.4))
        .chain(
            conn.db.enemy().iter()
                .map(|e| (ServerId::Enemy(e.id), e.entity, e.combat, "enemy.glb", 0.5))
        )
        .collect();

    // ── Local player: patch health, skip spawning ─────
    for (id, entity, _, _, _) in &rows {
        if let ServerId::Player(identity) = id {
            if Some(*identity) == my_id {
                if let Ok(mut health) = local_health.single_mut() {
                    health.current = entity.health;
                }
                seen.insert(id.clone());
            }
        }
    }

    // ── Patch or despawn existing remote entities ──────
    for (bevy_entity, id, mut world_entity, mut combat_stats) in &mut remote_entities {
        if let Some((_, entity, combat, _, _)) = rows.iter().find(|(rid, ..)| rid == id) {
            seen.insert(id.clone());
            *world_entity = entity.clone();
            *combat_stats = combat.clone();
        } else {
            commands.entity(bevy_entity).despawn_recursive();
        }
    }

    // ── Spawn new remote entities ──────────────────────
    for (id, entity, combat, mesh, radius) in &rows {
        if !seen.contains(id) {
            commands.spawn((
                id.clone(),
                entity.clone(),
                combat.clone(),
                Transform::from_xyz(entity.x, entity.y, entity.z),
                Health::new(entity.max_health),
                Mesh3d(asset_server.load(*mesh)),
                Collider::capsule(*radius, 1.0),
                RigidBody::Dynamic,
            ));
        }
    }

    // ── Combat events: same pattern ───────────────────
    let event_rows: HashSet<u64> = conn.db.combat_event().iter()
        .map(|e| e.id).collect();

    // Despawn events the server deleted
    for (bevy_entity, id) in &combat_events {
        if let ServerId::CombatEvent(eid) = id {
            if !event_rows.contains(eid) {
                commands.entity(bevy_entity).despawn();
            }
        }
    }

    // Spawn new events (On<Add, CombatEventData> observer triggers VFX)
    let existing_events: HashSet<u64> = combat_events.iter()
        .filter_map(|(_, id)| match id { ServerId::CombatEvent(eid) => Some(*eid), _ => None })
        .collect();

    for event in conn.db.combat_event().iter() {
        if !existing_events.contains(&event.id) {
            commands.spawn((
                ServerId::CombatEvent(event.id),
                CombatEventData {
                    damage: event.damage,
                    is_crit: event.is_crit,
                    attacker_x: event.attacker_x,
                    attacker_z: event.attacker_z,
                },
            ));
        }
    }
}
```

## Interpolation

The reconciler writes `WorldEntity` (target state). A separate generic system smoothly moves `Transform` toward the target each frame. Works for all entity types.

```rust
fn interpolate_synced_entities(
    time: Res<Time>,
    mut query: Query<(&WorldEntity, &mut Transform), With<ServerId>>,
) {
    let alpha = (time.delta_secs() * INTERPOLATION_SPEED).min(1.0);
    for (world_entity, mut transform) in &mut query {
        let target = Vec3::new(world_entity.x, world_entity.y, world_entity.z);
        transform.translation = transform.translation.lerp(target, alpha);
        transform.rotation = Quat::slerp(
            transform.rotation,
            Quat::from_rotation_y(world_entity.rotation_y),
            alpha,
        );
    }
}
```

## Damage Split

`combat/damage.rs` currently does VFX feedback AND health mutation in one observer with `cfg(feature)` imports.

Split `DamageDealt` into two observers:

- **`on_damage_feedback`** — always runs. Fires `HitLanded` for VFX/sound/shake. Applies knockback. Never touches Health.
- **`on_damage_apply`** — singleplayer only (`GameMode::Singleplayer`). Calls `health.take_damage()`, fires `Died`. In multiplayer, health is authoritative from the server via the reconciler.

Zero networking imports, zero `cfg(feature)` in combat/.

## Outbound (unchanged)

Client → server systems stay as-is:

- `send_local_position` — position + animation relay
- `send_attack_to_server` — attack reducer call
- `process_outbound_lag` — lag simulation
- `auto_connect` / `reap_dead_connections` — connection lifecycle
- `measure_ping` — ping tracking

## What Gets Deleted

| Old system | Replaced by |
|---|---|
| `spawn_remote_players` | reconciler (spawn) |
| `update_remote_players` | reconciler (patch) |
| `despawn_remote_players` | reconciler (despawn) |
| `buffer_inbound_updates` | reconciler (direct cache read) |
| `interpolate_positions` | `interpolate_synced_entities` (generic) |
| `sync_remote_health` | reconciler (patch) |
| `sync_local_health` | reconciler (local player branch) |
| `handle_remote_death` | reconciler (server deletes row → despawn) |
| `sync_npc_enemies` | reconciler (one `.chain()`) |
| `process_remote_combat_events` | reconciler (combat events as entities) |

Also deleted: `networking/player.rs` entirely, most of `networking/combat.rs`.

## Adding a New Synced Entity Type

1. Define the server table embedding `entity: WorldEntity` (and `combat: CombatStats` if it fights)
2. Add a `ServerId` variant
3. Add one `.chain()` in the reconciler's row collection
4. Reconciler body doesn't change

## Verification

1. `just check` — clippy, fmt, machete, web compilation pass
2. `just` — SP: spawn enemies, hit them, VFX plays, health drains, they die
3. `just mp` — MP: enemies visible on both clients, remote attacks show VFX, health syncs
4. `grep -r "networking" client/src/combat/` — zero results
5. `grep -r "cfg.*multiplayer" client/src/combat/` — zero results

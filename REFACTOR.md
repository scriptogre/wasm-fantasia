# Server-Driven Reconciliation Refactor

## Problem

The client has 10+ bespoke networking systems (spawn_remote_players, sync_npc_enemies, sync_remote_health, handle_remote_death, etc.) that each poll SpacetimeDB tables with their own change detection and build domain-specific entity bundles. `combat/` imports networking types behind `cfg(feature)`. Adding a new synced entity type requires new polling systems, new events, new observers.

## Goal

One generic reconciler. The SpacetimeDB client cache is a local mirror of server state. Each frame, the reconciler diffs the cache against the ECS and patches what's different. No callbacks, no message queues, no per-entity-type sync systems. Like HTMX's morph — server describes the world, client renders it.

## Server Tables

Flat tables. No embedded `#[spacetimedb::type]` structs — keeps DB access clean (`player.x` not `player.entity.x`). The reconciler constructs typed ECS components from flat fields in one place.

### Player

```rust
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: Identity,
    pub name: Option<String>,
    pub online: bool,
    pub last_update: i64,

    // position
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,

    // animation
    pub animation_state: String,
    pub attack_sequence: u32,
    pub attack_animation: String,

    // health
    pub health: f32,
    pub max_health: f32,

    // combat
    pub attack_damage: f32,
    pub crit_chance: f32,
    pub crit_multiplier: f32,
    pub attack_range: f32,
    pub attack_arc: f32,
    pub knockback_force: f32,
    pub attack_speed: f32,
    pub last_attack_time: i64,
}
```

### Enemy

Same shape minus identity/name/online. Enemies that can fight like players get the full combat stat set. Simpler enemies (e.g. training dummies) just leave stats at zero.

```rust
#[spacetimedb::table(name = enemy, public)]
pub struct Enemy {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub enemy_type: String,

    // position
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,

    // animation
    pub animation_state: String,

    // health
    pub health: f32,
    pub max_health: f32,

    // combat
    pub attack_damage: f32,
    pub attack_range: f32,
    pub attack_speed: f32,
    pub last_attack_time: i64,
}
```

### CombatEvent

Ephemeral hit notification for VFX. Stripped to the minimum the client needs: where it happened, how much, was it a crit. No attacker identity, no target identity, no polymorphic FK.

```rust
#[spacetimedb::table(name = combat_event, public)]
pub struct CombatEvent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub damage: f32,
    pub is_crit: bool,
    pub timestamp: i64,
}
```

### ActiveEffect

Satellite table for dynamic, scriptable effects (buffs, debuffs, DoTs). Replaces hardcoded `stacks`/`stack_decay`/`last_hit_time` columns on Player. When Rhai/Lua scripting arrives, scripts insert/update/delete rows here. The reconciler picks them up automatically — just another table.

```rust
#[spacetimedb::table(name = active_effect, public)]
pub struct ActiveEffect {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner: Identity,
    pub effect_type: String,
    pub magnitude: f32,
    pub duration: f32,
    pub timestamp: i64,
}
```

The server's combat resolver queries this table for the attacker, finds relevant effects (e.g. `"stacking_damage"`), and applies them. No hardcoded buff columns on entity tables.

**Limitation:** `owner` is `Identity`, so this only supports player-owned effects. When enemies need buffs, either add `owner_enemy_id: Option<u64>` or split into `player_effect`/`enemy_effect` tables. Cross that bridge when we get there.

## Client Components

The reconciler constructs these typed ECS components from flat DB fields. They exist only on the client — the server has no knowledge of them.

```rust
/// Target position for interpolation. The reconciler writes this;
/// a separate system lerps Transform toward it each frame.
#[derive(Component, Clone)]
pub struct WorldEntity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,
}

/// Offensive combat stats. Queried by combat resolution.
#[derive(Component, Clone)]
pub struct CombatStats {
    pub attack_damage: f32,
    pub crit_chance: f32,
    pub crit_multiplier: f32,
    pub attack_range: f32,
    pub attack_arc: f32,
    pub knockback_force: f32,
    pub attack_speed: f32,
    pub last_attack_time: i64,
}
```

`Health { current, max }` already exists as its own component — no new type needed.

## ServerId

Links an ECS entity to a server table row. The only per-type enum in the system.

```rust
#[derive(Component, Clone, Hash, Eq, PartialEq)]
pub enum ServerId {
    Player(Identity),
    Enemy(u64),
    CombatEvent(u64),
}
```

## The Reconciler

One function. Every table is another `.chain()`. The reconciler constructs `WorldEntity`, `CombatStats`, and `Health` from flat DB fields.

The local player already exists (spawned by the player module). The reconciler patches its health from the server row and skips spawning a duplicate. Remote entities are spawned, patched, or despawned to match server state.

Combat events are just another table. Spawning an entity with `CombatEventData` triggers an `On<Add, CombatEventData>` observer for VFX. The server deletes stale rows, the reconciler despawns the entity next frame.

```rust
fn reconcile(
    conn: Res<SpacetimeDbConnection>,
    mut remote_entities: Query<(Entity, &ServerId, &mut WorldEntity, &mut CombatStats, &mut Health)>,
    mut local_health: Query<&mut Health, (With<Player>, Without<ServerId>)>,
    mut combat_events: Query<(Entity, &ServerId), With<CombatEventData>>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    let my_id = conn.conn.try_identity();
    let mut seen = HashSet::new();

    // ── Collect all entity tables into one flat list ───
    // Each row: (id, position, combat stats, health, max_health, mesh, collider_radius)
    let rows: Vec<(ServerId, WorldEntity, CombatStats, f32, f32, &str, f32)> =
        conn.db.player().iter()
            .filter(|p| p.online)
            .map(|p| (
                ServerId::Player(p.identity),
                WorldEntity { x: p.x, y: p.y, z: p.z, rotation_y: p.rotation_y },
                CombatStats {
                    attack_damage: p.attack_damage,
                    crit_chance: p.crit_chance,
                    crit_multiplier: p.crit_multiplier,
                    attack_range: p.attack_range,
                    attack_arc: p.attack_arc,
                    knockback_force: p.knockback_force,
                    attack_speed: p.attack_speed,
                    last_attack_time: p.last_attack_time,
                },
                p.health, p.max_health,
                "player.glb", 0.4,
            ))
        .chain(
            conn.db.enemy().iter()
                .map(|e| (
                    ServerId::Enemy(e.id),
                    WorldEntity { x: e.x, y: e.y, z: e.z, rotation_y: e.rotation_y },
                    CombatStats {
                        attack_damage: e.attack_damage,
                        crit_chance: 0.0,
                        crit_multiplier: 1.0,
                        attack_range: e.attack_range,
                        attack_arc: 180.0,
                        knockback_force: 0.0,
                        attack_speed: e.attack_speed,
                        last_attack_time: e.last_attack_time,
                    },
                    e.health, e.max_health,
                    "enemy.glb", 0.5,
                ))
        )
        .collect();

    // ── Local player: patch health, skip spawning ─────
    for (id, _, _, health, _, _, _) in &rows {
        if let ServerId::Player(identity) = id {
            if Some(*identity) == my_id {
                if let Ok(mut h) = local_health.single_mut() {
                    h.current = *health;
                }
                seen.insert(id.clone());
            }
        }
    }

    // ── Patch or despawn existing remote entities ──────
    for (bevy_entity, id, mut world_entity, mut combat_stats, mut health) in &mut remote_entities {
        if let Some((_, we, cs, hp, max_hp, _, _)) = rows.iter().find(|(rid, ..)| rid == id) {
            seen.insert(id.clone());
            *world_entity = we.clone();
            *combat_stats = cs.clone();
            health.current = *hp;
            health.max = *max_hp;
        } else {
            commands.entity(bevy_entity).despawn_recursive();
        }
    }

    // ── Spawn new remote entities ──────────────────────
    for (id, world_entity, combat_stats, health, max_health, mesh, radius) in &rows {
        if !seen.contains(id) {
            commands.spawn((
                id.clone(),
                world_entity.clone(),
                combat_stats.clone(),
                Transform::from_xyz(world_entity.x, world_entity.y, world_entity.z),
                Health::new(*max_health).with_current(*health),
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
                    x: event.x,
                    y: event.y,
                    z: event.z,
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
| `stacks`/`stack_decay` columns on Player | `ActiveEffect` satellite table |

Also deleted: `networking/player.rs` entirely, most of `networking/combat.rs`.

## Adding a New Synced Entity Type

1. Add the server table with position + health + combat columns as needed
2. Add a `ServerId` variant
3. Add one `.chain()` in the reconciler's row collection, constructing `WorldEntity`/`CombatStats`/`Health` from the flat fields
4. Reconciler body doesn't change

## Adding a New Dynamic Effect

1. Server inserts an `ActiveEffect` row (e.g. `effect_type: "stacking_damage"`)
2. Server's combat resolver queries `active_effect` table for the attacker
3. Client reconciler picks up the row automatically
4. When Rhai/Lua scripting exists, scripts manage these rows instead of hardcoded Rust

## Verification

1. `just check` — clippy, fmt, machete, web compilation pass
2. `just` — SP: spawn enemies, hit them, VFX plays, health drains, they die
3. `just` (with two clients) — MP: enemies visible on both clients, remote attacks show VFX, health syncs
4. `grep -r "networking" client/src/combat/` — zero results
5. `grep -r "cfg.*multiplayer" client/src/combat/` — zero results

//! Networking combat module: attack relay, health sync, enemy sync, and remote death handling.

use super::generated::attack_hit_reducer::attack_hit;
use super::generated::combat_event_table::CombatEventTableAccess;
use super::generated::npc_enemy_table::NpcEnemyTableAccess;
use super::generated::player_table::PlayerTableAccess;
use super::generated::respawn_reducer::respawn;
use super::generated::spawn_enemies_reducer::spawn_enemies;
use super::player::RemotePlayer;
use super::SpacetimeDbConnection;
use crate::combat::{AttackHit, Combatant, DamageEvent, Enemy, Health, HitFeedback, PlayerCombatant};
use crate::models::Player as LocalPlayer;
use crate::rules::{Stat, Stats};
use avian3d::prelude::{Collider, LockedAxes, Mass, RigidBody};
use bevy::prelude::*;
use spacetimedb_sdk::{DbContext, Table};

/// Observer: when local player's attack connects, notify the server.
pub fn send_attack_to_server(
    on: On<AttackHit>,
    players: Query<(), With<PlayerCombatant>>,
    conn: Res<SpacetimeDbConnection>,
) {
    // Only relay attacks from the local player
    if players.get(on.event().attacker).is_ok() {
        if let Err(e) = conn.conn.reducers.attack_hit() {
            warn!("Failed to send attack_hit: {:?}", e);
        }
    }
}

/// Sync remote player health from server state.
pub fn sync_remote_health(
    conn: Res<SpacetimeDbConnection>,
    mut query: Query<(&RemotePlayer, &mut Health, &mut Stats)>,
) {
    for (rp, mut health, mut stats) in query.iter_mut() {
        let Some(player) = conn.conn.db.player().identity().find(&rp.identity) else {
            continue;
        };

        health.current = player.health;
        health.max = player.max_health;
        stats.set(Stat::Health, player.health);
        stats.set(Stat::MaxHealth, player.max_health);
    }
}

/// Sync local player health from server (we may have been hit by another player).
pub fn sync_local_health(
    conn: Res<SpacetimeDbConnection>,
    mut query: Query<(&mut Health, &mut Stats), With<LocalPlayer>>,
) {
    let Some(our_identity) = conn.conn.try_identity() else {
        return;
    };
    let Some(server_player) = conn.conn.db.player().identity().find(&our_identity) else {
        return;
    };

    let Ok((mut health, mut stats)) = query.single_mut() else {
        return;
    };

    health.current = server_player.health;
    health.max = server_player.max_health;
    stats.set(Stat::Health, server_player.health);
    stats.set(Stat::MaxHealth, server_player.max_health);
}

/// Hide remote players at 0 HP instead of despawning (server owns death).
pub fn handle_remote_death(
    conn: Res<SpacetimeDbConnection>,
    mut query: Query<(&RemotePlayer, &mut Visibility)>,
) {
    for (rp, mut visibility) in query.iter_mut() {
        let Some(player) = conn.conn.db.player().identity().find(&rp.identity) else {
            continue;
        };

        if player.health <= 0.0 {
            *visibility = Visibility::Hidden;
        } else {
            *visibility = Visibility::Inherited;
        }
    }
}

/// Auto-respawn when local player dies (calls server respawn reducer).
pub fn request_respawn_on_death(
    conn: Res<SpacetimeDbConnection>,
    query: Query<&Health, With<LocalPlayer>>,
) {
    let Ok(health) = query.single() else {
        return;
    };

    if health.is_dead() {
        if let Err(e) = conn.conn.reducers.respawn() {
            warn!("Failed to send respawn: {:?}", e);
        }
    }
}

// =============================================================================
// NPC Enemy sync
// =============================================================================

/// Marker for server-synced NPC enemies (links to NpcEnemy.id in server table).
#[derive(Component)]
pub struct ServerEnemy {
    pub server_id: u64,
}

/// Spawn/update/despawn NPC enemies from the server's npc_enemy table.
pub fn sync_npc_enemies(
    conn: Res<SpacetimeDbConnection>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut existing: Query<(Entity, &ServerEnemy, &mut Transform, &mut Health)>,
) {
    // Collect server enemy state for comparison
    let server_enemies: Vec<_> = conn.conn.db.npc_enemy().iter().collect();

    // Update existing enemies or despawn if removed from server
    for (entity, se, mut transform, mut health) in existing.iter_mut() {
        if let Some(server_e) = server_enemies.iter().find(|e| e.id == se.server_id) {
            // Update position and health from server
            transform.translation = Vec3::new(server_e.x, server_e.y, server_e.z);
            health.current = server_e.health;
            health.max = server_e.max_health;
        } else {
            // Enemy removed from server (dead) — despawn locally
            commands.entity(entity).despawn();
        }
    }

    // Spawn new enemies that don't have local entities yet
    let existing_ids: Vec<u64> = existing.iter().map(|(_, se, _, _)| se.server_id).collect();

    for enemy in &server_enemies {
        if existing_ids.contains(&enemy.id) {
            continue;
        }

        // Each enemy needs its own material for hit flash
        let enemy_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.2),
            ..default()
        });

        let enemy_mesh = meshes.add(Capsule3d::new(0.5, 1.0));

        commands.spawn((
            Name::new(format!("ServerEnemy_{}", enemy.id)),
            ServerEnemy {
                server_id: enemy.id,
            },
            Transform::from_xyz(enemy.x, enemy.y, enemy.z),
            Mesh3d(enemy_mesh),
            MeshMaterial3d(enemy_material),
            Health::new(enemy.max_health),
            Enemy,
            Combatant,
            Stats::new()
                .with(Stat::MaxHealth, enemy.max_health)
                .with(Stat::Health, enemy.health),
            Collider::capsule(0.5, 1.0),
            RigidBody::Dynamic,
            LockedAxes::ROTATION_LOCKED,
            Mass(500.0),
        ));
    }
}

// =============================================================================
// Remote combat VFX
// =============================================================================

/// Tracks which CombatEvent IDs have been processed to avoid double-firing VFX.
#[derive(Resource, Default)]
pub struct CombatEventTracker {
    last_processed_id: u64,
}

/// Poll the combat_event table for new events from remote players.
/// Fires DamageEvent so existing VFX observers (damage numbers, hit flash, impact particles) trigger.
/// Local player's own hits are skipped (already handled by the local attack pipeline).
pub fn process_remote_combat_events(
    conn: Res<SpacetimeDbConnection>,
    mut tracker: ResMut<CombatEventTracker>,
    remote_players: Query<(Entity, &RemotePlayer)>,
    server_enemies: Query<(Entity, &ServerEnemy)>,
    local_player: Query<Entity, With<LocalPlayer>>,
    mut commands: Commands,
) {
    let our_identity = conn.conn.try_identity();

    for event in conn.conn.db.combat_event().iter() {
        if event.id <= tracker.last_processed_id {
            continue;
        }
        tracker.last_processed_id = event.id;

        // Skip our own attacks — VFX already fired locally
        if Some(event.attacker) == our_identity {
            continue;
        }

        // Resolve target entity
        let target = if let Some(ref target_identity) = event.target_player {
            if Some(*target_identity) == our_identity {
                local_player.single().ok()
            } else {
                remote_players
                    .iter()
                    .find(|(_, rp)| rp.identity == *target_identity)
                    .map(|(e, _)| e)
            }
        } else if let Some(npc_id) = event.target_npc_id {
            server_enemies
                .iter()
                .find(|(_, se)| se.server_id == npc_id)
                .map(|(e, _)| e)
        } else {
            None
        };

        let Some(target_entity) = target else {
            continue;
        };

        // Resolve attacker entity (remote player)
        let Some((source_entity, _)) = remote_players
            .iter()
            .find(|(_, rp)| rp.identity == event.attacker)
        else {
            continue;
        };

        // Fire DamageEvent — on_damage already skips health modification for remote entities,
        // and it triggers HitEvent which drives all VFX observers.
        commands.trigger(DamageEvent {
            source: source_entity,
            target: target_entity,
            damage: event.damage,
            force: Vec3::ZERO,
            is_crit: event.is_crit,
            feedback: HitFeedback::standard(event.is_crit),
        });
    }
}

/// Send spawn_enemies request to server (replaces local-only enemy spawn in multiplayer).
pub fn server_spawn_enemies(
    conn: &SpacetimeDbConnection,
    pos: Vec3,
    forward: Vec3,
) {
    if let Err(e) = conn.conn.reducers.spawn_enemies(
        pos.x,
        pos.y,
        pos.z,
        forward.x,
        forward.z,
    ) {
        warn!("Failed to send spawn_enemies: {:?}", e);
    }
}

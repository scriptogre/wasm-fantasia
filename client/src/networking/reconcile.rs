//! Event-driven server→client reconciliation via SpacetimeDB table callbacks.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use spacetimedb_sdk::{DbContext, Identity, Table};
use wasm_fantasia_shared::combat::EnemyBehaviorKind;

use super::SpacetimeDbConnection;
use super::generated::enemy_table::EnemyTableAccess;
use super::generated::enemy_type::Enemy as ServerEnemy;
use super::generated::player_table::PlayerTableAccess;
use super::generated::player_type::Player as ServerPlayer;
use crate::combat::{Combatant, Enemy, EnemyBehavior, Health};
use crate::models::Player as LocalPlayer;
use crate::player::RemotePlayer;
use crate::rules::{Stat, Stats};

// =============================================================================
// Components
// =============================================================================

/// Links an ECS entity to a server table row.
#[derive(Component, Clone, Hash, Eq, PartialEq, Debug)]
pub enum ServerId {
    Player(Identity),
    Enemy(u64),
}

/// Target position for interpolation. Written by reconciler, consumed by interpolation system.
#[derive(Component, Clone, Debug)]
pub struct WorldEntity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,
    /// Server velocity — used to extrapolate between subscription updates.
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub velocity_z: f32,
}

/// Tracks the last received server snapshot so the interpolation system can
/// detect when new data arrives and extrapolate over the full elapsed time.
#[derive(Component, Debug)]
pub struct ServerSnapshot {
    pub position: Vec3,
    pub velocity: Vec3,
    pub received_at: f32,
}

impl Default for ServerSnapshot {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            received_at: 0.0,
        }
    }
}

/// Offensive combat stats synced from server.
#[derive(Component, Clone, Debug)]
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

/// Marker for data carried by combat event entities.
#[derive(Component)]
pub struct CombatEventData {
    pub damage: f32,
    pub is_crit: bool,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Server-synced animation state for remote players.
#[derive(Component, Clone, Debug, Default)]
pub struct RemotePlayerState {
    pub animation_state: String,
    pub attack_sequence: u32,
    pub attack_animation: String,
}

// =============================================================================
// Snapshot types
// =============================================================================

pub(super) struct EnemySnapshot {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub velocity_z: f32,
    pub animation_state: String,
    pub health: f32,
    pub max_health: f32,
}

pub(super) struct PlayerSnapshot {
    pub identity: Identity,
    pub online: bool,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,
    pub animation_state: String,
    pub attack_sequence: u32,
    pub attack_animation: String,
    pub health: f32,
    pub max_health: f32,
}

impl From<&ServerEnemy> for EnemySnapshot {
    fn from(e: &ServerEnemy) -> Self {
        Self {
            id: e.id,
            x: e.x,
            y: e.y,
            z: e.z,
            rotation_y: e.rotation_y,
            velocity_x: e.velocity_x,
            velocity_y: e.velocity_y,
            velocity_z: e.velocity_z,
            animation_state: e.animation_state.clone(),
            health: e.health,
            max_health: e.max_health,
        }
    }
}

impl From<&ServerPlayer> for PlayerSnapshot {
    fn from(p: &ServerPlayer) -> Self {
        Self {
            identity: p.identity,
            online: p.online,
            x: p.x,
            y: p.y,
            z: p.z,
            rotation_y: p.rotation_y,
            animation_state: p.animation_state.clone(),
            attack_sequence: p.attack_sequence,
            attack_animation: p.attack_animation.clone(),
            health: p.health,
            max_health: p.max_health,
        }
    }
}

// =============================================================================
// Events & Resources
// =============================================================================

pub(super) enum DbEvent {
    EnemyInsert { enemy: EnemySnapshot },
    EnemyUpdate { id: u64, new: EnemySnapshot },
    EnemyDelete { id: u64 },
    PlayerInsert { player: PlayerSnapshot },
    PlayerUpdate { identity: Identity, new: PlayerSnapshot },
    PlayerDelete { identity: Identity },
    CombatEventInsert { damage: f32, is_crit: bool, x: f32, y: f32, z: f32 },
}

#[derive(Resource, Clone, Default)]
pub struct DbEventQueue(pub(super) Arc<Mutex<Vec<DbEvent>>>);

#[derive(Resource, Default)]
pub struct ServerEntityMap {
    pub enemies: HashMap<u64, Entity>,
    pub players: HashMap<Identity, Entity>,
    synced: bool,
}

// =============================================================================
// Systems
// =============================================================================

/// Drains the event queue populated by SpacetimeDB table callbacks and applies
/// each change to the ECS. Most frames the queue is empty and this returns
/// immediately — only frames where the server pushed changes do real work.
pub(super) fn drain_db_events(
    conn: Res<SpacetimeDbConnection>,
    queue: Res<DbEventQueue>,
    mut entity_map: ResMut<ServerEntityMap>,
    mut local_health: Query<(&mut Health, &mut Stats), With<LocalPlayer>>,
    mut remote_enemies: Query<
        (&mut WorldEntity, &mut Health, Option<&mut EnemyBehavior>),
        (With<Enemy>, Without<LocalPlayer>, Without<RemotePlayer>),
    >,
    mut remote_players: Query<
        (&mut WorldEntity, &mut Health, &mut RemotePlayerState),
        (With<RemotePlayer>, Without<LocalPlayer>, Without<Enemy>),
    >,
    mut commands: Commands,
) {
    let mut events = {
        let mut lock = queue.0.lock().unwrap();
        std::mem::take(&mut *lock)
    };

    // Seed from existing subscription data when re-entering gameplay
    // after a keep-alive disconnect (entities despawned but connection alive).
    if events.is_empty() && !entity_map.synced && conn.conn.try_identity().is_some() {
        entity_map.synced = true;
        for enemy in conn.conn.db.enemy().iter() {
            events.push(DbEvent::EnemyInsert {
                enemy: (&enemy).into(),
            });
        }
        for player in conn.conn.db.player().iter() {
            events.push(DbEvent::PlayerInsert {
                player: (&player).into(),
            });
        }
    }

    if events.is_empty() {
        return;
    }

    if !entity_map.synced {
        entity_map.synced = true;
    }

    let my_id = conn.conn.try_identity();

    for event in events {
        match event {
            DbEvent::EnemyInsert { enemy } => {
                let entity = commands
                    .spawn((
                        Name::new(format!("Enemy_{}", enemy.id)),
                        ServerId::Enemy(enemy.id),
                        WorldEntity {
                            x: enemy.x,
                            y: enemy.y,
                            z: enemy.z,
                            rotation_y: enemy.rotation_y,
                            velocity_x: enemy.velocity_x,
                            velocity_y: enemy.velocity_y,
                            velocity_z: enemy.velocity_z,
                        },
                        ServerSnapshot::default(),
                        Transform::from_xyz(enemy.x, enemy.y, enemy.z),
                        Health::new(enemy.max_health),
                        Enemy,
                        Combatant,
                        Stats::new()
                            .with(Stat::MaxHealth, enemy.max_health)
                            .with(Stat::Health, enemy.health),
                    ))
                    .id();
                entity_map.enemies.insert(enemy.id, entity);
            }
            DbEvent::EnemyUpdate { id, new } => {
                if let Some(&entity) = entity_map.enemies.get(&id) {
                    if let Ok((mut world, mut health, behavior)) =
                        remote_enemies.get_mut(entity)
                    {
                        *world = WorldEntity {
                            x: new.x,
                            y: new.y,
                            z: new.z,
                            rotation_y: new.rotation_y,
                            velocity_x: new.velocity_x,
                            velocity_y: new.velocity_y,
                            velocity_z: new.velocity_z,
                        };
                        health.current = new.health;
                        health.max = new.max_health;

                        if let Some(mut behavior) = behavior {
                            let kind = EnemyBehaviorKind::parse_str(&new.animation_state);
                            let new_behavior = match kind {
                                EnemyBehaviorKind::Idle => EnemyBehavior::Idle,
                                EnemyBehaviorKind::Chase => EnemyBehavior::Chase,
                                EnemyBehaviorKind::Attack => EnemyBehavior::Attack,
                            };
                            if *behavior != new_behavior {
                                *behavior = new_behavior;
                            }
                        }
                    }
                }
            }
            DbEvent::EnemyDelete { id } => {
                if let Some(entity) = entity_map.enemies.remove(&id) {
                    commands.entity(entity).despawn();
                }
            }
            DbEvent::PlayerInsert { player } => {
                if my_id == Some(player.identity) {
                    if let Ok((mut health, mut stats)) = local_health.single_mut() {
                        health.current = player.health;
                        health.max = player.max_health;
                        stats.set(Stat::Health, player.health);
                        stats.set(Stat::MaxHealth, player.max_health);
                    }
                } else if player.online {
                    let entity = commands
                        .spawn((
                            Name::new(format!("RemotePlayer_{:?}", player.identity)),
                            ServerId::Player(player.identity),
                            WorldEntity {
                                x: player.x,
                                y: player.y,
                                z: player.z,
                                rotation_y: player.rotation_y,
                                velocity_x: 0.0,
                                velocity_y: 0.0,
                                velocity_z: 0.0,
                            },
                            ServerSnapshot::default(),
                            Transform::from_xyz(player.x, player.y, player.z),
                            Health::new(player.max_health),
                            RemotePlayer,
                            RemotePlayerState {
                                animation_state: player.animation_state,
                                attack_sequence: player.attack_sequence,
                                attack_animation: player.attack_animation,
                            },
                        ))
                        .id();
                    entity_map.players.insert(player.identity, entity);
                }
            }
            DbEvent::PlayerUpdate { identity, new } => {
                if my_id == Some(identity) {
                    if let Ok((mut health, mut stats)) = local_health.single_mut() {
                        health.current = new.health;
                        health.max = new.max_health;
                        stats.set(Stat::Health, new.health);
                        stats.set(Stat::MaxHealth, new.max_health);
                    }
                } else if new.online {
                    if let Some(&entity) = entity_map.players.get(&identity) {
                        if let Ok((mut world, mut health, mut state)) =
                            remote_players.get_mut(entity)
                        {
                            *world = WorldEntity {
                                x: new.x,
                                y: new.y,
                                z: new.z,
                                rotation_y: new.rotation_y,
                                velocity_x: 0.0,
                                velocity_y: 0.0,
                                velocity_z: 0.0,
                            };
                            health.current = new.health;
                            health.max = new.max_health;
                            state.animation_state = new.animation_state;
                            state.attack_sequence = new.attack_sequence;
                            state.attack_animation = new.attack_animation;
                        }
                    } else {
                        // Player came online — spawn
                        let entity = commands
                            .spawn((
                                Name::new(format!("RemotePlayer_{:?}", identity)),
                                ServerId::Player(identity),
                                WorldEntity {
                                    x: new.x,
                                    y: new.y,
                                    z: new.z,
                                    rotation_y: new.rotation_y,
                                    velocity_x: 0.0,
                                    velocity_y: 0.0,
                                    velocity_z: 0.0,
                                },
                                ServerSnapshot::default(),
                                Transform::from_xyz(new.x, new.y, new.z),
                                Health::new(new.max_health),
                                RemotePlayer,
                                RemotePlayerState {
                                    animation_state: new.animation_state,
                                    attack_sequence: new.attack_sequence,
                                    attack_animation: new.attack_animation,
                                },
                            ))
                            .id();
                        entity_map.players.insert(identity, entity);
                    }
                } else {
                    // Player went offline — despawn
                    if let Some(entity) = entity_map.players.remove(&identity) {
                        commands.entity(entity).despawn();
                    }
                }
            }
            DbEvent::PlayerDelete { identity } => {
                if let Some(entity) = entity_map.players.remove(&identity) {
                    commands.entity(entity).despawn();
                }
            }
            DbEvent::CombatEventInsert {
                damage,
                is_crit,
                x,
                y,
                z,
            } => {
                commands.spawn((
                    CombatEventData {
                        damage,
                        is_crit,
                        x,
                        y,
                        z,
                    },
                    Transform::from_xyz(x, y, z),
                ));
            }
        }
    }
}

pub(super) fn reset_entity_map(mut commands: Commands) {
    commands.insert_resource(ServerEntityMap::default());
}

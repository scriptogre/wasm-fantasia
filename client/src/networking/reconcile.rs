//! Server→client entity reconciliation: diffs SpacetimeDB cache against ECS.

use bevy::prelude::*;
use std::collections::HashSet;


use spacetimedb_sdk::{DbContext, Table};
use wasm_fantasia_shared::combat::EnemyBehaviorKind;

use super::SpacetimeDbConnection;
use super::generated::combat_event_table::CombatEventTableAccess;
use super::generated::enemy_table::EnemyTableAccess;
use super::generated::player_table::PlayerTableAccess;
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
    Player(spacetimedb_sdk::Identity),
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
// Resources
// =============================================================================

/// Tracks which CombatEvent IDs have been processed.
#[derive(Resource, Default)]
pub struct CombatEventTracker {
    last_processed_id: u64,
}

// =============================================================================
// Systems
// =============================================================================

/// One system that diffs the SpacetimeDB client cache against the ECS each frame.
/// Spawns, patches, or despawns entities to match server state.
pub(super) fn reconcile(
    conn: Res<SpacetimeDbConnection>,
    mut remote_entities: Query<
        (
            Entity,
            &ServerId,
            &mut WorldEntity,
            &mut Health,
            Option<&mut EnemyBehavior>,
            Option<&mut RemotePlayerState>,
        ),
        Without<LocalPlayer>,
    >,
    mut local_health: Query<(&mut Health, &mut Stats), With<LocalPlayer>>,
    mut tracker: ResMut<CombatEventTracker>,
    mut commands: Commands,
) {
    let my_id = conn.conn.try_identity();
    let mut seen = HashSet::new();

    // ── Collect all entity tables into one flat list ───
    struct Row {
        id: ServerId,
        world: WorldEntity,
        health: f32,
        max_health: f32,
        animation_state: String,
        attack_sequence: u32,
        attack_animation: String,
    }

    let rows: Vec<Row> = conn
        .conn
        .db
        .player()
        .iter()
        .filter(|p| p.online)
        .map(|p| Row {
            id: ServerId::Player(p.identity),
            world: WorldEntity {
                x: p.x,
                y: p.y,
                z: p.z,
                rotation_y: p.rotation_y,
                velocity_x: 0.0,
                velocity_y: 0.0,
                velocity_z: 0.0,
            },
            health: p.health,
            max_health: p.max_health,
            animation_state: p.animation_state.clone(),
            attack_sequence: p.attack_sequence,
            attack_animation: p.attack_animation.clone(),
        })
        .chain(conn.conn.db.enemy().iter().map(|e| Row {
            id: ServerId::Enemy(e.id),
            world: WorldEntity {
                x: e.x,
                y: e.y,
                z: e.z,
                rotation_y: e.rotation_y,
                velocity_x: e.velocity_x,
                velocity_y: e.velocity_y,
                velocity_z: e.velocity_z,
            },
            health: e.health,
            max_health: e.max_health,
            animation_state: e.animation_state.clone(),
            attack_sequence: 0,
            attack_animation: String::new(),
        }))
        .collect();

    // ── Local player: patch health, skip spawning ─────
    for row in &rows {
        if let ServerId::Player(identity) = &row.id {
            if Some(*identity) == my_id {
                if let Ok((mut health, mut stats)) = local_health.single_mut() {
                    health.current = row.health;
                    health.max = row.max_health;
                    stats.set(Stat::Health, row.health);
                    stats.set(Stat::MaxHealth, row.max_health);
                }
                seen.insert(row.id.clone());
            }
        }
    }

    // ── Patch or despawn existing remote entities ──────
    for (bevy_entity, id, mut world_entity, mut health, enemy_behavior, remote_state) in
        &mut remote_entities
    {
        if let Some(row) = rows.iter().find(|r| &r.id == id) {
            seen.insert(id.clone());
            *world_entity = row.world.clone();
            health.current = row.health;
            health.max = row.max_health;

            // Patch enemy behavior from server animation_state
            if let Some(mut behavior) = enemy_behavior {
                let kind = EnemyBehaviorKind::parse_str(&row.animation_state);
                let new_behavior = match kind {
                    EnemyBehaviorKind::Idle => EnemyBehavior::Idle,
                    EnemyBehaviorKind::Chase => EnemyBehavior::Chase,
                    EnemyBehaviorKind::Attack => EnemyBehavior::Attack,
                };
                if *behavior != new_behavior {
                    *behavior = new_behavior;
                }
            }

            // Patch remote player animation state
            if let Some(mut state) = remote_state {
                state.animation_state = row.animation_state.clone();
                state.attack_sequence = row.attack_sequence;
                state.attack_animation = row.attack_animation.clone();
            }
        } else {
            commands.entity(bevy_entity).despawn();
        }
    }

    // ── Spawn new remote entities ──────────────────────
    for row in &rows {
        if seen.contains(&row.id) {
            continue;
        }

        let is_enemy = matches!(&row.id, ServerId::Enemy(_));
        let name = match &row.id {
            ServerId::Player(id) => format!("RemotePlayer_{id:?}"),
            ServerId::Enemy(id) => format!("Enemy_{id}"),
        };

        if is_enemy {
            // Enemy: On<Add, Enemy> observer attaches GLTF model + animations
            commands.spawn((
                Name::new(name),
                row.id.clone(),
                row.world.clone(),
                ServerSnapshot::default(),
                Transform::from_xyz(row.world.x, row.world.y, row.world.z),
                Health::new(row.max_health),
                Enemy,
                Combatant,
                Stats::new()
                    .with(Stat::MaxHealth, row.max_health)
                    .with(Stat::Health, row.health),
            ));
        } else {
            // Remote player: On<Add, RemotePlayer> observer attaches GLTF model + animations
            commands.spawn((
                Name::new(name),
                row.id.clone(),
                row.world.clone(),
                ServerSnapshot::default(),
                Transform::from_xyz(row.world.x, row.world.y, row.world.z),
                Health::new(row.max_health),
                RemotePlayer,
                RemotePlayerState {
                    animation_state: row.animation_state.clone(),
                    attack_sequence: row.attack_sequence,
                    attack_animation: row.attack_animation.clone(),
                },
            ));
        }
    }

    // ── Combat events ─────────────────────────────────
    for event in conn.conn.db.combat_event().iter() {
        if event.id <= tracker.last_processed_id {
            continue;
        }
        tracker.last_processed_id = event.id;

        commands.spawn((
            CombatEventData {
                damage: event.damage,
                is_crit: event.is_crit,
                x: event.x,
                y: event.y,
                z: event.z,
            },
            Transform::from_xyz(event.x, event.y, event.z),
        ));
    }
}

//! Server→client entity reconciliation: diffs SpacetimeDB cache against ECS.

use bevy::prelude::*;
use std::collections::HashSet;

use avian3d::prelude::{Collider, LockedAxes, Mass, RigidBody};
use spacetimedb_sdk::{DbContext, Table};
use wasm_fantasia_shared::combat::EnemyBehaviorKind;

use super::SpacetimeDbConnection;
use super::generated::combat_event_table::CombatEventTableAccess;
use super::generated::enemy_table::EnemyTableAccess;
use super::generated::player_table::PlayerTableAccess;
use crate::combat::{Combatant, Enemy, EnemyBehavior, Health};
use crate::models::Player as LocalPlayer;
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
        ),
        Without<LocalPlayer>,
    >,
    mut local_health: Query<(&mut Health, &mut Stats), With<LocalPlayer>>,
    mut tracker: ResMut<CombatEventTracker>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
            },
            health: p.health,
            max_health: p.max_health,
            animation_state: String::new(),
        })
        .chain(conn.conn.db.enemy().iter().map(|e| Row {
            id: ServerId::Enemy(e.id),
            world: WorldEntity {
                x: e.x,
                y: e.y,
                z: e.z,
                rotation_y: e.rotation_y,
            },
            health: e.health,
            max_health: e.max_health,
            animation_state: e.animation_state.clone(),
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
    for (bevy_entity, id, mut world_entity, mut health, enemy_behavior) in &mut remote_entities {
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
                Transform::from_xyz(row.world.x, row.world.y, row.world.z),
                Health::new(row.max_health),
                Enemy,
                Combatant,
                Stats::new()
                    .with(Stat::MaxHealth, row.max_health)
                    .with(Stat::Health, row.health),
            ));
        } else {
            // Remote player: capsule mesh (TODO: player model for remotes)
            let material = materials.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.6, 1.0),
                ..default()
            });
            let mesh = meshes.add(Capsule3d::new(0.5, 1.0));

            commands.spawn((
                Name::new(name),
                row.id.clone(),
                row.world.clone(),
                Transform::from_xyz(row.world.x, row.world.y, row.world.z),
                Health::new(row.max_health),
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Collider::capsule(0.5, 1.0),
                RigidBody::Dynamic,
                LockedAxes::ROTATION_LOCKED,
                Mass(500.0),
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

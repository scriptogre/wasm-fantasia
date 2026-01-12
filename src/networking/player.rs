//! Player-specific networking logic

use bevy::prelude::*;
use super::{SpacetimeDbConnection, PositionSyncTimer};
use super::generated::player_table::PlayerTableAccess;
use super::generated::update_position_reducer::update_position;
use spacetimedb_sdk::{Table, DbContext};
use crate::models::Player as LocalPlayer;
use crate::asset_loading::Models;

/// Component marking an entity as a remote player
#[derive(Component, Clone, Debug)]
pub struct RemotePlayer {
    pub identity: spacetimedb_sdk::Identity,
}

/// Component storing the target position for interpolation
#[derive(Component, Clone, Debug)]
pub struct InterpolatedPosition {
    pub target: Vec3,
    pub target_rotation: f32,
}

/// System to spawn remote players when they appear in the database
pub fn spawn_remote_players(
    conn: Res<SpacetimeDbConnection>,
    models: Option<Res<Models>>,
    gltf_assets: Res<Assets<Gltf>>,
    mut commands: Commands,
    existing_players: Query<&RemotePlayer>,
) {
    let Some(models) = models else { return };
    let Some(gltf) = gltf_assets.get(&models.player) else {
        return;
    };

    let our_identity = conn.conn.try_identity();

    for player in conn.conn.db.player().iter() {
        // Skip local player
        if Some(player.identity) == our_identity {
            continue;
        }

        // Skip offline players
        if !player.online {
            continue;
        }

        // Skip already spawned
        if existing_players.iter().any(|rp| rp.identity == player.identity) {
            continue;
        }

        info!(
            "Spawning remote player: {:?} at ({}, {}, {})",
            player.name, player.x, player.y, player.z
        );

        let mesh = SceneRoot(gltf.scenes[0].clone());

        commands
            .spawn((
                RemotePlayer { identity: player.identity },
                InterpolatedPosition {
                    target: Vec3::new(player.x, player.y, player.z),
                    target_rotation: player.rot_y,
                },
                Transform::from_xyz(player.x, player.y, player.z)
                    .with_rotation(Quat::from_rotation_y(player.rot_y)),
                InheritedVisibility::default(),
            ))
            .with_children(|parent| {
                // Spawn character mesh as child, offset down like local player
                parent.spawn((Transform::from_xyz(0.0, -1.0, 0.0), mesh));
            });
    }
}

/// System to update remote player positions from SpacetimeDB
pub fn update_remote_players(
    conn: Res<SpacetimeDbConnection>,
    mut query: Query<(&RemotePlayer, &mut InterpolatedPosition)>,
) {
    for (rp, mut interp) in query.iter_mut() {
        if let Some(player) = conn.conn.db.player().identity().find(&rp.identity) {
            interp.target = Vec3::new(player.x, player.y, player.z);
            interp.target_rotation = player.rot_y;
        }
    }
}

/// System to despawn remote players when they disconnect
pub fn despawn_remote_players(
    conn: Res<SpacetimeDbConnection>,
    mut commands: Commands,
    query: Query<(Entity, &RemotePlayer)>,
) {
    for (entity, rp) in query.iter() {
        let should_despawn = conn.conn
            .db
            .player()
            .identity()
            .find(&rp.identity)
            .map(|p| !p.online)
            .unwrap_or(true);

        if should_despawn {
            info!("Despawning remote player: {:?}", rp.identity);
            commands.entity(entity).despawn();
        }
    }
}

/// System to interpolate remote player positions
pub fn interpolate_positions(
    mut query: Query<(&mut Transform, &InterpolatedPosition), With<RemotePlayer>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    const LERP_SPEED: f32 = 10.0;

    for (mut transform, interp) in query.iter_mut() {
        transform.translation = transform.translation.lerp(interp.target, LERP_SPEED * dt);

        let current_y = transform.rotation.to_euler(EulerRot::YXZ).0;
        let rot_lerp = current_y + (interp.target_rotation - current_y) * (LERP_SPEED * dt);
        transform.rotation = Quat::from_rotation_y(rot_lerp);
    }
}

/// System to send local player position to the server
pub fn send_local_position(
    conn: Res<SpacetimeDbConnection>,
    mut timer: ResMut<PositionSyncTimer>,
    time: Res<Time>,
    query: Query<&Transform, With<LocalPlayer>>,
) {
    timer.timer.tick(time.delta());

    if !timer.timer.just_finished() {
        return;
    }

    let Ok(transform) = query.single() else {
        return;
    };

    let pos = transform.translation;
    let rot_y = transform.rotation.to_euler(EulerRot::YXZ).0;

    // TODO: Get actual animation state from player
    let anim_state = "Idle".to_string();

    if let Err(e) = conn.conn.reducers.update_position(pos.x, pos.y, pos.z, rot_y, anim_state) {
        warn!("Failed to send position update: {:?}", e);
    }
}

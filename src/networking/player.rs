//! Player-specific networking logic

use bevy::prelude::*;
use super::{SpacetimeDbConnection, PositionSyncTimer, LagSimulator, LagBuffers, PendingOutboundUpdate, BufferedInboundState};
use super::generated::player_table::PlayerTableAccess;
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

/// System to buffer incoming remote player updates from SpacetimeDB
/// This runs BEFORE update_remote_players to populate the delay buffer
pub fn buffer_inbound_updates(
    conn: Res<SpacetimeDbConnection>,
    lag: Res<LagSimulator>,
    mut buffers: ResMut<LagBuffers>,
) {
    // If no lag simulation, skip buffering (update_remote_players will read directly)
    if lag.inbound_delay_ms == 0 && lag.packet_loss_chance == 0.0 {
        return;
    }

    let now = std::time::Instant::now();

    for player in conn.conn.db.player().iter() {
        // Skip offline players
        if !player.online {
            continue;
        }

        // Check for packet loss simulation
        if lag.packet_loss_chance > 0.0 && rand::random::<f32>() < lag.packet_loss_chance {
            continue; // Drop this update
        }

        buffers.inbound_buffer.insert(player.identity, BufferedInboundState {
            x: player.x, y: player.y, z: player.z, rot_y: player.rot_y,
            received_at: now,
        });
    }
}

/// System to update remote player positions from SpacetimeDB
pub fn update_remote_players(
    conn: Res<SpacetimeDbConnection>,
    lag: Res<LagSimulator>,
    buffers: Res<LagBuffers>,
    mut query: Query<(&RemotePlayer, &mut InterpolatedPosition)>,
) {
    let now = std::time::Instant::now();

    for (rp, mut interp) in query.iter_mut() {
        if lag.inbound_delay_ms == 0 && lag.packet_loss_chance == 0.0 {
            // No lag - read directly from DB
            if let Some(player) = conn.conn.db.player().identity().find(&rp.identity) {
                interp.target = Vec3::new(player.x, player.y, player.z);
                interp.target_rotation = player.rot_y;
            }
        } else {
            // Read from buffer if delay has elapsed
            if let Some(state) = buffers.inbound_buffer.get(&rp.identity) {
                let elapsed = now.duration_since(state.received_at).as_millis() as u64;
                if elapsed >= lag.inbound_delay_ms {
                    interp.target = Vec3::new(state.x, state.y, state.z);
                    interp.target_rotation = state.rot_y;
                }
                // If delay hasn't elapsed, keep old target (no update yet)
            }
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
    lag: Res<LagSimulator>,
    mut buffers: ResMut<LagBuffers>,
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

    if lag.outbound_delay_ms == 0 && lag.packet_loss_chance == 0.0 {
        // No lag - this will be sent immediately by process_outbound_lag
        buffers.outbound_queue.push(PendingOutboundUpdate {
            x: pos.x, y: pos.y, z: pos.z, rot_y, anim_state,
            send_at: std::time::Instant::now(),
        });
    } else {
        // Queue with delay
        let send_at = std::time::Instant::now() + std::time::Duration::from_millis(lag.outbound_delay_ms);
        buffers.outbound_queue.push(PendingOutboundUpdate {
            x: pos.x, y: pos.y, z: pos.z, rot_y, anim_state,
            send_at,
        });
    }
}

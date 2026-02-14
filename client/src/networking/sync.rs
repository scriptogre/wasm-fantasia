//! Outbound position relay, entity interpolation, and ping measurement.

use bevy::prelude::*;
use spacetimedb_sdk::DbContext;
use web_time::Instant;

use super::SpacetimeDbConnection;
use super::generated::player_table::PlayerTableAccess;
use super::generated::update_position_reducer::update_position;
use super::reconcile::{ServerId, ServerSnapshot, WorldEntity};
use crate::combat::AttackState;
use crate::models::Player as LocalPlayer;
use crate::player::Animation;

const INTERPOLATION_SPEED: f32 = 12.0;
const GRAVITY: f32 = -9.81;

// =============================================================================
// Resources
// =============================================================================

/// Tracks round-trip time by comparing position send timestamps against server acks.
#[derive(Resource, Default)]
pub struct PingTracker {
    pub last_send: Option<Instant>,
    pub last_seen_update: i64,
    pub smoothed_rtt_ms: f32,
    pub last_ack: Option<Instant>,
}

/// Timer for position sync rate limiting.
#[derive(Resource)]
pub struct PositionSyncTimer {
    pub timer: Timer,
}

impl Default for PositionSyncTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.05, TimerMode::Repeating),
        }
    }
}

// =============================================================================
// Systems
// =============================================================================

/// Dead reckoning: detect new server snapshots, extrapolate position using
/// velocity + gravity over the full time since the snapshot, then lerp toward
/// the extrapolated target. This produces smooth continuous falling/movement
/// even when subscription updates arrive at ~10Hz.
pub(super) fn interpolate_synced_entities(
    time: Res<Time>,
    mut query: Query<(&WorldEntity, &mut ServerSnapshot, &mut Transform), With<ServerId>>,
) {
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    let alpha = (dt * INTERPOLATION_SPEED).min(1.0);

    for (world_entity, mut snapshot, mut transform) in &mut query {
        let server_pos = Vec3::new(world_entity.x, world_entity.y, world_entity.z);
        let server_vel = Vec3::new(
            world_entity.velocity_x,
            world_entity.velocity_y,
            world_entity.velocity_z,
        );

        // Detect when the reconciler wrote new data from a subscription update.
        // Between updates the cache returns identical values, so an exact
        // comparison is reliable.
        if server_pos != snapshot.position {
            snapshot.position = server_pos;
            snapshot.velocity = server_vel;
            snapshot.received_at = now;
        }

        // Time elapsed since the last real server update.
        let elapsed = now - snapshot.received_at;

        // Extrapolate: project the snapshot forward by the full elapsed time
        // using the velocity at that snapshot plus gravitational acceleration.
        // This predicts where the entity *should* be right now, producing
        // smooth parabolic arcs between the ~10Hz subscription updates.
        //
        // Only apply gravity when the entity has vertical velocity — otherwise
        // grounded entities sink through the floor as elapsed grows unbounded
        // (position-based snapshot detection doesn't reset when standing still).
        let gravity_term = if snapshot.velocity.y.abs() > 0.01 {
            0.5 * GRAVITY * elapsed * elapsed
        } else {
            0.0
        };
        let target =
            snapshot.position + snapshot.velocity * elapsed + Vec3::new(0.0, gravity_term, 0.0);

        transform.translation = transform.translation.lerp(target, alpha);
        transform.rotation = Quat::slerp(
            transform.rotation,
            Quat::from_rotation_y(world_entity.rotation_y),
            alpha,
        );
    }
}

/// Send local player position to the server at a fixed rate.
pub(super) fn send_local_position(
    conn: Res<SpacetimeDbConnection>,
    mut timer: ResMut<PositionSyncTimer>,
    mut ping: ResMut<PingTracker>,
    time: Res<Time>,
    query: Query<(&Transform, &LocalPlayer, Option<&AttackState>), With<LocalPlayer>>,
) {
    timer.timer.tick(time.delta());
    if !timer.timer.just_finished() {
        return;
    }

    let Ok((transform, player, attack_state)) = query.single() else {
        return;
    };

    let pos = transform.translation;
    let rotation_y = transform.rotation.to_euler(EulerRot::YXZ).0;
    let animation_state = player.animation_state.server_name().to_string();

    let (attack_sequence, attack_animation) = if let Some(attack) = attack_state {
        let anim = if attack.is_crit {
            Animation::MeleeHook
        } else if attack.attack_count % 2 == 1 {
            Animation::PunchJab
        } else {
            Animation::PunchCross
        };
        (attack.attack_count, anim.clip_name().to_string())
    } else {
        (0, String::new())
    };

    ping.last_send = Some(Instant::now());

    if let Err(e) = conn.conn.reducers.update_position(
        pos.x,
        pos.y,
        pos.z,
        rotation_y,
        animation_state,
        attack_sequence,
        attack_animation,
    ) {
        warn!("Failed to send position update: {:?}", e);
    }
}

pub(super) fn measure_ping(conn: Res<SpacetimeDbConnection>, mut tracker: ResMut<PingTracker>) {
    let Some(identity) = conn.conn.try_identity() else {
        return;
    };
    let Some(player) = conn.conn.db.player().identity().find(&identity) else {
        return;
    };

    if player.last_update != tracker.last_seen_update {
        tracker.last_seen_update = player.last_update;
        tracker.last_ack = Some(Instant::now());

        if let Some(send_time) = tracker.last_send.take() {
            let rtt_ms = send_time.elapsed().as_secs_f32() * 1000.0;
            if tracker.smoothed_rtt_ms <= 0.0 {
                tracker.smoothed_rtt_ms = rtt_ms;
            } else {
                tracker.smoothed_rtt_ms = tracker.smoothed_rtt_ms * 0.8 + rtt_ms * 0.2;
            }
        }
    }
}

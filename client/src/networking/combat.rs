//! Outbound combat networking: attack relay, respawn, enemy spawn requests.

use super::SpacetimeDbConnection;
use super::generated::attack_hit_reducer::attack_hit;
use super::generated::ground_pound_hit_reducer::ground_pound_hit;
use super::generated::landing_aoe_hit_reducer::landing_aoe_hit;
use super::generated::respawn_reducer::respawn;
use super::generated::clear_enemies_reducer::clear_enemies;
use super::generated::spawn_enemies_reducer::spawn_enemies;
use crate::combat::{AttackIntent, Health, PlayerCombatant};
use crate::models::Player as LocalPlayer;
use crate::player::control::{GroundPoundImpact, LandingImpact};
use bevy::prelude::*;

/// Observer: when local player's attack connects, notify the server.
pub fn send_attack_to_server(
    on: On<AttackIntent>,
    players: Query<(), With<PlayerCombatant>>,
    conn: Option<Res<SpacetimeDbConnection>>,
) {
    let Some(conn) = conn else { return };
    if players.get(on.event().attacker).is_ok() {
        if let Err(e) = conn.conn.reducers.attack_hit() {
            warn!("Failed to send attack_hit: {:?}", e);
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

/// Send clear_enemies request to server.
pub fn server_clear_enemies(conn: &SpacetimeDbConnection) {
    if let Err(e) = conn.conn.reducers.clear_enemies() {
        warn!("Failed to send clear_enemies: {:?}", e);
    }
}

/// Observer: when ground pound lands, notify the server.
pub fn send_ground_pound_to_server(
    on: On<GroundPoundImpact>,
    conn: Option<Res<SpacetimeDbConnection>>,
) {
    let Some(conn) = conn else { return };
    let event = on.event();
    if let Err(e) = conn.conn.reducers.ground_pound_hit(
        event.position.x,
        event.position.y,
        event.position.z,
    ) {
        warn!("Failed to send ground_pound_hit: {:?}", e);
    }
}

/// Observer: when a high-velocity landing occurs, notify the server.
pub fn send_landing_aoe_to_server(
    on: On<LandingImpact>,
    conn: Option<Res<SpacetimeDbConnection>>,
) {
    let Some(conn) = conn else { return };
    let event = on.event();
    if event.velocity_y < wasm_fantasia_shared::combat::landing_aoe::MIN_VELOCITY {
        return;
    }
    if let Err(e) = conn.conn.reducers.landing_aoe_hit(
        event.velocity_y,
        event.position.x,
        event.position.y,
        event.position.z,
    ) {
        warn!("Failed to send landing_aoe_hit: {:?}", e);
    }
}

/// Send spawn_enemies request to server.
pub fn server_spawn_enemies(conn: &SpacetimeDbConnection, pos: Vec3, forward: Vec3) {
    if let Err(e) = conn
        .conn
        .reducers
        .spawn_enemies(pos.x, pos.y, pos.z, forward.x, forward.z)
    {
        warn!("Failed to send spawn_enemies: {:?}", e);
    }
}

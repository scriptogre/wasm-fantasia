//! Outbound combat networking: attack relay, respawn, enemy spawn requests.

use super::SpacetimeDbConnection;
use super::generated::attack_hit_reducer::attack_hit;
use super::generated::respawn_reducer::respawn;
use super::generated::spawn_enemies_reducer::spawn_enemies;
use crate::combat::{AttackIntent, Health, PlayerCombatant};
use crate::models::Player as LocalPlayer;
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

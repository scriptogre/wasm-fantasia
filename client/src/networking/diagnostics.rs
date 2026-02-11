//! Server diagnostics resource — networking reads SpacetimeDB tables, other
//! modules read this resource. Prevents domain modules from importing networking.

use bevy::prelude::*;
use spacetimedb_sdk::{DbContext, Table};

use super::generated::combat_event_table::CombatEventTableAccess;
use super::generated::enemy_table::EnemyTableAccess;
use super::generated::player_table::PlayerTableAccess;
use super::SpacetimeDbConnection;
use crate::combat::{Health, PlayerCombatant};

#[derive(Default)]
pub struct PlayerDiagnostic {
    pub name: String,
    pub is_you: bool,
    pub online: bool,
    pub health: f32,
    pub max_health: f32,
}

#[derive(Default)]
pub struct EventDiagnostic {
    pub damage: f32,
    pub is_crit: bool,
    pub x: f32,
    pub z: f32,
}

#[derive(Resource, Default)]
pub struct ServerDiagnostics {
    pub players: Vec<PlayerDiagnostic>,
    pub enemy_alive: usize,
    pub enemy_dead: usize,
    pub recent_events: Vec<EventDiagnostic>,
    /// (local_health, server_health) when desynced by > 0.1
    pub health_desync: Option<(f32, f32)>,
    pub connected: bool,
}

pub(super) fn update_server_diagnostics(
    conn: Res<SpacetimeDbConnection>,
    mut diag: ResMut<ServerDiagnostics>,
    player_health: Query<&Health, With<PlayerCombatant>>,
) {
    let our_id = conn.conn.try_identity();
    diag.connected = true;

    // Players
    let mut players: Vec<PlayerDiagnostic> = conn
        .conn
        .db
        .player()
        .iter()
        .map(|p| PlayerDiagnostic {
            name: p.name.clone().unwrap_or_else(|| "?".to_string()),
            is_you: Some(p.identity) == our_id,
            online: p.online,
            health: p.health,
            max_health: p.max_health,
        })
        .collect();
    players.sort_by_key(|p| {
        let you = if p.is_you { 0 } else { 1 };
        let online = if p.online { 0 } else { 1 };
        (online, you)
    });
    diag.players = players;

    // Enemies
    let enemies: Vec<_> = conn.conn.db.enemy().iter().collect();
    diag.enemy_alive = enemies.iter().filter(|e| e.health > 0.0).count();
    diag.enemy_dead = enemies.len() - diag.enemy_alive;

    // Recent combat events
    let mut events: Vec<_> = conn.conn.db.combat_event().iter().collect();
    events.sort_by_key(|e| e.id);
    diag.recent_events = events
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(|e| EventDiagnostic {
            damage: e.damage,
            is_crit: e.is_crit,
            x: e.x,
            z: e.z,
        })
        .collect();

    // Desync check
    diag.health_desync = None;
    if let Ok(local_hp) = player_health.single() {
        if let Some(id) = our_id {
            if let Some(sp) = conn.conn.db.player().identity().find(&id) {
                let delta = (local_hp.current - sp.health).abs();
                if delta > 0.1 {
                    diag.health_desync = Some((local_hp.current, sp.health));
                }
            }
        }
    }
}

pub(super) fn clear_server_diagnostics(mut diag: ResMut<ServerDiagnostics>) {
    *diag = ServerDiagnostics::default();
}

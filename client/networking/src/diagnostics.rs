//! Server diagnostics resource — networking reads SpacetimeDB tables, other
//! modules read this resource. Prevents domain modules from importing networking.

use bevy::prelude::*;
use spacetimedb_sdk::{DbContext, Table};

use super::SpacetimeDbConnection;
use super::generated::combat_event_table::CombatEventTableAccess;
use super::generated::enemy_table::EnemyTableAccess;
use super::generated::player_table::PlayerTableAccess;
use game_client_models::combat::{Health, PlayerCombatant};

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
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    diag.connected = true;

    // Throttle expensive table scans to twice per second. The desync check
    // (single player lookup) still runs every frame for responsiveness.
    *timer += time.delta_secs();
    if *timer >= 0.5 {
        *timer = 0.0;

        let our_id = conn.conn.try_identity();

        // Players (small table — collect is fine)
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

        // Enemies — count directly without allocating a Vec
        let mut alive = 0usize;
        let mut total = 0usize;
        for e in conn.conn.db.enemy().iter() {
            total += 1;
            if e.health > 0.0 {
                alive += 1;
            }
        }
        diag.enemy_alive = alive;
        diag.enemy_dead = total - alive;

        // Recent combat events — track last 3 by max id without sorting all
        let mut max_id = [0u64; 3];
        let mut top3: [Option<EventDiagnostic>; 3] = [None, None, None];
        for e in conn.conn.db.combat_event().iter() {
            // Find the slot with the smallest id that this event can replace
            if let Some(i) = (0..3).filter(|&i| e.id > max_id[i]).min_by_key(|&i| max_id[i]) {
                max_id[i] = e.id;
                top3[i] = Some(EventDiagnostic {
                    damage: e.damage,
                    is_crit: e.is_crit,
                    x: e.x,
                    z: e.z,
                });
            }
        }
        diag.recent_events = top3.into_iter().flatten().collect();
    }

    // Desync check — cheap single-player lookup, runs every frame
    diag.health_desync = None;
    if let Ok(local_hp) = player_health.single() {
        if let Some(id) = conn.conn.try_identity() {
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

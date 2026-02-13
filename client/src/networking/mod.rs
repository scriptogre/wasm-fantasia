//! SpacetimeDB multiplayer networking module

use bevy::prelude::*;

use crate::models::{GameplayCleanup, Screen, ServerTarget};

pub mod combat;
mod connection;
mod diagnostics;
pub mod generated;
#[cfg(not(target_arch = "wasm32"))]
pub mod local_server;
mod reconcile;
mod sync;

pub use connection::{ReconnectTimer, try_connect};
pub use diagnostics::ServerDiagnostics;
pub use generated::{DbConnection, Player, Reducer};
pub use reconcile::{
    CombatEventData, CombatStats, RemotePlayerState, ServerId, ServerSnapshot, WorldEntity,
};
pub use sync::PingTracker;

// =============================================================================
// Resources
// =============================================================================

/// SpacetimeDB connection resource.
#[derive(Resource)]
pub struct SpacetimeDbConnection {
    pub conn: DbConnection,
}

/// SpacetimeDB configuration resource.
#[derive(Resource, Clone, Debug)]
pub struct SpacetimeDbConfig {
    pub uri: String,
    pub module_name: String,
}

impl Default for SpacetimeDbConfig {
    fn default() -> Self {
        Self {
            uri: default_uri(),
            module_name: "wasm-fantasia".to_string(),
        }
    }
}

/// On WASM, derive the SpacetimeDB URI from the page's location.
fn default_uri() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(location) = web_sys::window().map(|w| w.location()) {
            if let Some(uri) = location.search().ok().and_then(|s| {
                s.trim_start_matches('?')
                    .split('&')
                    .find_map(|p| p.strip_prefix("stdb="))
                    .map(String::from)
            }) {
                return uri;
            }
            if let Some(host) = location.hostname().ok().filter(|h| !h.is_empty()) {
                let scheme = match location.protocol().ok().as_deref() {
                    Some("https:") => "wss",
                    _ => "ws",
                };
                // HTTPS (production): use default port 443 — Caddy routes
                // /database/* to SpacetimeDB on the same domain.
                // HTTP (dev): SpacetimeDB runs on port 3000 locally.
                return match scheme {
                    "wss" => format!("wss://{host}"),
                    _ => format!("ws://{host}:3000"),
                };
            }
        }
    }
    "ws://127.0.0.1:3000".to_string()
}

/// Persists the SpacetimeDB auth token across reconnects.
#[derive(Resource, Default, Clone)]
pub struct SpacetimeDbToken(pub std::sync::Arc<std::sync::Mutex<Option<String>>>);

/// Run condition: true when a SpacetimeDB connection is live (any mode).
pub fn is_server_connected(conn: Option<Res<SpacetimeDbConnection>>) -> bool {
    conn.is_some()
}

pub const STALE_THRESHOLD_SECS: f32 = 3.0;

// =============================================================================
// Plugin
// =============================================================================

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(local_server::plugin);

        app.init_resource::<SpacetimeDbConfig>()
            .init_resource::<SpacetimeDbToken>()
            .init_resource::<connection::ReconnectTimer>()
            .init_resource::<sync::PositionSyncTimer>()
            .init_resource::<sync::PingTracker>()
            .init_resource::<reconcile::CombatEventTracker>()
            .init_resource::<diagnostics::ServerDiagnostics>()
            .add_systems(
                OnEnter(Screen::Connecting),
                connection::reset_reconnect_timer.run_if(resource_exists::<ServerTarget>),
            )
            .add_systems(Update, connection::auto_connect)
            .add_systems(
                OnExit(Screen::Connecting),
                connection::cleanup_connecting_exit,
            )
            .add_systems(
                OnExit(Screen::Gameplay),
                (
                    connection::disconnect_from_spacetimedb,
                    connection::remove_server_target,
                )
                    .run_if(is_server_connected)
                    .before(GameplayCleanup),
            );

        app.add_observer(combat::send_attack_to_server).add_systems(
            Update,
            (
                connection::reap_dead_connections.run_if(resource_exists::<SpacetimeDbConnection>),
                connection::handle_connection_events
                    .run_if(resource_exists::<SpacetimeDbConnection>),
                reconcile::reconcile.run_if(resource_exists::<SpacetimeDbConnection>),
                sync::interpolate_synced_entities.run_if(resource_exists::<SpacetimeDbConnection>),
                sync::send_local_position.run_if(resource_exists::<SpacetimeDbConnection>),
                combat::request_respawn_on_death.run_if(resource_exists::<SpacetimeDbConnection>),
                sync::measure_ping.run_if(resource_exists::<SpacetimeDbConnection>),
                diagnostics::update_server_diagnostics
                    .run_if(resource_exists::<SpacetimeDbConnection>),
            ),
        );

        app.add_systems(
            Update,
            diagnostics::clear_server_diagnostics
                .run_if(not(resource_exists::<SpacetimeDbConnection>)),
        );
    }
}

//! Connection lifecycle: connect, reconnect, handshake, disconnect, cleanup.

use bevy::prelude::*;
use spacetimedb_sdk::DbContext;
use web_time::Instant;

use super::generated::join_game_reducer::join_game;
use super::generated::leave_game_reducer::leave_game;
use super::{DbConnection, SpacetimeDbConfig, SpacetimeDbConnection, SpacetimeDbToken};
use crate::models::{GameMode, Screen, ServerTarget};

#[cfg(not(target_arch = "wasm32"))]
use super::local_server;

const HANDSHAKE_TIMEOUT_SECS: f32 = 5.0;
const RECONNECT_INTERVAL_SECS: f32 = 2.0;

// =============================================================================
// Resources
// =============================================================================

#[derive(Resource)]
pub struct ReconnectTimer(pub Timer);

impl Default for ReconnectTimer {
    fn default() -> Self {
        let mut timer = Timer::from_seconds(RECONNECT_INTERVAL_SECS, TimerMode::Repeating);
        // Pre-tick to almost done so the first real tick fires immediately
        timer.tick(std::time::Duration::from_secs_f32(
            RECONNECT_INTERVAL_SECS - 0.01,
        ));
        Self(timer)
    }
}

#[derive(Resource)]
pub(super) struct HandshakeStart(Instant);

// =============================================================================
// Systems
// =============================================================================

macro_rules! connection_builder {
    ($uri:expr, $module_name:expr, $token:expr, $is_solo:expr) => {{
        let token_store = $token.clone();
        let stored = $token.lock().unwrap().clone();
        let is_solo = $is_solo;
        DbConnection::builder()
            .with_uri($uri)
            .with_module_name($module_name)
            .with_token(stored)
            .on_connect(move |conn, identity, token| {
                info!("Connected to SpacetimeDB with identity: {:?}", identity);
                *token_store.lock().unwrap() = Some(token.to_string());

                let world_id = if is_solo {
                    identity.to_hex().to_string()
                } else {
                    "shared".to_string()
                };

                if let Err(e) = conn
                    .reducers
                    .join_game(Some("Player".to_string()), world_id.clone())
                {
                    error!("Failed to call join_game: {:?}", e);
                }
                conn.subscription_builder().subscribe([
                    format!("SELECT * FROM player WHERE world_id = '{world_id}'"),
                    format!("SELECT * FROM enemy WHERE world_id = '{world_id}'"),
                    format!("SELECT * FROM combat_event WHERE world_id = '{world_id}'"),
                    "SELECT * FROM active_effect".to_string(),
                ]);
            })
            .on_connect_error(|_ctx, err| {
                error!("Failed to connect to SpacetimeDB: {:?}", err);
            })
            .on_disconnect(|ctx, err| {
                warn!("Disconnected from SpacetimeDB: {:?}", err);
                let _ = ctx.reducers.leave_game();
            })
    }};
}

pub fn try_connect(
    uri: &str,
    module_name: &str,
    token: &SpacetimeDbToken,
    is_solo: bool,
) -> Option<SpacetimeDbConnection> {
    info!("Attempting SpacetimeDB connection to {uri}...");
    match connection_builder!(uri, module_name, token.0, is_solo).build() {
        Ok(conn) => {
            info!("Connection initiated — waiting for handshake");
            Some(SpacetimeDbConnection { conn })
        }
        Err(e) => {
            warn!("SpacetimeDB connection failed: {e:?}");
            None
        }
    }
}

pub(super) fn reset_reconnect_timer(mut timer: ResMut<ReconnectTimer>) {
    *timer = ReconnectTimer::default();
}

/// Clean up when leaving the Connecting screen without a completed handshake.
pub(super) fn cleanup_connecting_exit(
    conn: Option<Res<SpacetimeDbConnection>>,
    mut commands: Commands,
    #[cfg(not(target_arch = "wasm32"))] server_state: Option<
        Res<local_server::LocalServerState>,
    >,
) {
    if conn
        .as_ref()
        .is_some_and(|c| c.conn.try_identity().is_some())
    {
        return; // heading to Gameplay — keep everything
    }

    if let Some(conn) = conn {
        let _ = conn.conn.disconnect();
        commands.remove_resource::<SpacetimeDbConnection>();
        commands.remove_resource::<HandshakeStart>();
    }

    // Preserve a Ready server so the player can resume from the title screen.
    // Only remove the server if it failed or is still starting — stale state
    // would block the next auto_connect attempt.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let is_ready = server_state
            .is_some_and(|s| matches!(*s, local_server::LocalServerState::Ready));
        if !is_ready {
            commands.remove_resource::<local_server::LocalServer>();
            commands.remove_resource::<local_server::LocalServerState>();
        }
    }
}

pub(super) fn disconnect_from_spacetimedb(
    conn: Option<Res<SpacetimeDbConnection>>,
    mut commands: Commands,
    mut ping: ResMut<super::PingTracker>,
    mut mode: ResMut<GameMode>,
    #[cfg(not(target_arch = "wasm32"))] server_state: Option<
        Res<local_server::LocalServerState>,
    >,
) {
    // In singleplayer with a running local server, keep the connection alive
    // so the player can resume from the title screen without losing world state.
    #[cfg(not(target_arch = "wasm32"))]
    if *mode == GameMode::Singleplayer
        && server_state.is_some_and(|s| matches!(*s, local_server::LocalServerState::Ready))
    {
        return;
    }

    if let Some(conn) = conn {
        if let Err(e) = conn.conn.disconnect() {
            warn!("SpacetimeDB disconnect error: {e:?}");
        }
        commands.remove_resource::<SpacetimeDbConnection>();
    }
    *ping = super::PingTracker::default();
    *mode = GameMode::default();
}

pub(super) fn remove_server_target(mut commands: Commands) {
    commands.remove_resource::<ServerTarget>();
}

pub(super) fn auto_connect(
    config: Res<SpacetimeDbConfig>,
    token: Res<SpacetimeDbToken>,
    mode: Res<GameMode>,
    mut timer: ResMut<ReconnectTimer>,
    time: Res<Time>,
    mut commands: Commands,
    state: Res<State<Screen>>,
    server_target: Option<Res<ServerTarget>>,
    conn: Option<Res<SpacetimeDbConnection>>,
    #[cfg(not(target_arch = "wasm32"))] local_server_state: Option<
        Res<local_server::LocalServerState>,
    >,
) {
    let Some(target) = server_target else { return };
    if !matches!(state.get(), Screen::Connecting | Screen::Gameplay) || conn.is_some() {
        return;
    }

    // For local servers, wait until the server is ready before attempting connection
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(ref ls_state) = local_server_state {
        if !matches!(**ls_state, local_server::LocalServerState::Ready) {
            return;
        }
    }

    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    // Derive URI from ServerTarget — never from mutable config
    let uri = match target.as_ref() {
        ServerTarget::Local { port } => format!("ws://127.0.0.1:{port}"),
        ServerTarget::Remote { uri } => uri.clone(),
    };
    let is_solo = *mode != GameMode::Multiplayer;
    if let Some(conn) = try_connect(&uri, &config.module_name, &token, is_solo) {
        commands.insert_resource(conn);
        commands.insert_resource(HandshakeStart(Instant::now()));
        info!("auto_connect: connection initiated");
    } else {
        warn!("auto_connect: try_connect returned None");
    }
}

pub(super) fn reap_dead_connections(
    conn: Option<Res<SpacetimeDbConnection>>,
    start: Option<Res<HandshakeStart>>,
    mut commands: Commands,
) {
    let Some(conn) = conn else { return };

    if !conn.conn.is_active() {
        warn!("Connection lost — cleaning up for retry");
        commands.remove_resource::<SpacetimeDbConnection>();
        commands.remove_resource::<HandshakeStart>();
        return;
    }

    if conn.conn.try_identity().is_some() {
        commands.remove_resource::<HandshakeStart>();
        return;
    }

    if let Some(start) = start {
        if start.0.elapsed().as_secs_f32() > HANDSHAKE_TIMEOUT_SECS {
            warn!("Handshake timeout — dropping stale connection for retry");
            let _ = conn.conn.disconnect();
            commands.remove_resource::<SpacetimeDbConnection>();
            commands.remove_resource::<HandshakeStart>();
        }
    }
}

pub(super) fn handle_connection_events(conn: Res<SpacetimeDbConnection>) {
    if let Err(e) = conn.conn.frame_tick() {
        warn!("frame_tick error: {e:?}");
    }
}

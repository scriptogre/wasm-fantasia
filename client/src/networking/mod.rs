//! SpacetimeDB multiplayer networking module

use bevy::prelude::*;
use spacetimedb_sdk::DbContext;

use crate::models::{is_multiplayer_mode, GameMode, Screen};

pub mod combat;
pub mod generated;
pub mod player;

pub use generated::{DbConnection, Player, Reducer};
pub use player::*;

use generated::join_game_reducer::join_game;
use generated::leave_game_reducer::leave_game;
use generated::player_table::PlayerTableAccess;
use generated::update_position_reducer::update_position;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use web_time::Instant;

/// SpacetimeDB connection resource
#[derive(Resource)]
pub struct SpacetimeDbConnection {
    pub conn: DbConnection,
}

/// SpacetimeDB configuration resource
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
/// Protocol is inferred from the page (http→ws, https→wss).
/// Full override via `?stdb=<uri>` query parameter.
/// Native defaults to localhost.
fn default_uri() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(location) = web_sys::window().map(|w| w.location()) {
            // Full override via ?stdb=wss://host:port
            if let Some(uri) = location.search().ok().and_then(|s| {
                s.trim_start_matches('?')
                    .split('&')
                    .find_map(|p| p.strip_prefix("stdb="))
                    .map(String::from)
            }) {
                return uri;
            }

            // Derive from page origin
            if let Some(host) = location.hostname().ok().filter(|h| !h.is_empty()) {
                let scheme = match location.protocol().ok().as_deref() {
                    Some("https:") => "wss",
                    _ => "ws",
                };
                let port = match scheme {
                    "wss" => 8443,
                    _ => 3000,
                };
                return format!("{scheme}://{host}:{port}");
            }
        }
    }
    "ws://127.0.0.1:3000".to_string()
}

/// Persists the SpacetimeDB auth token across reconnects so the server recognizes the same player.
#[derive(Resource, Default, Clone)]
pub struct SpacetimeDbToken(pub Arc<Mutex<Option<String>>>);

/// Tracks round-trip time by comparing position send timestamps against server acks.
#[derive(Resource, Default)]
pub struct PingTracker {
    /// Timestamp of the last position send
    pub last_send: Option<Instant>,
    /// The `last_update` field from our player row when we last checked
    pub last_seen_update: i64,
    /// Exponentially smoothed RTT in milliseconds
    pub smoothed_rtt_ms: f32,
    /// When we last received any acknowledgment from the server
    pub last_ack: Option<Instant>,
}

/// How long without a server ack before we consider the connection stale.
pub const STALE_THRESHOLD_SECS: f32 = 3.0;

/// How long to wait for a handshake before considering the connection dead.
const HANDSHAKE_TIMEOUT_SECS: f32 = 5.0;

/// How often to retry connecting (seconds).
const RECONNECT_INTERVAL_SECS: f32 = 2.0;

/// Timer for auto-reconnect attempts.
#[derive(Resource)]
pub struct ReconnectTimer(pub Timer);

impl Default for ReconnectTimer {
    fn default() -> Self {
        // Fire immediately on first tick, then repeat
        let mut timer = Timer::from_seconds(RECONNECT_INTERVAL_SECS, TimerMode::Repeating);
        timer.tick(std::time::Duration::from_secs_f32(RECONNECT_INTERVAL_SECS));
        Self(timer)
    }
}

/// Plugin for SpacetimeDB networking
pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpacetimeDbConfig>()
            .init_resource::<SpacetimeDbToken>()
            .init_resource::<ReconnectTimer>()
            .init_resource::<PositionSyncTimer>()
            .init_resource::<LagSimulator>()
            .init_resource::<LagBuffers>()
            .init_resource::<PingTracker>()
            .init_resource::<combat::CombatEventTracker>()
            .add_systems(
                Update,
                auto_connect.run_if(
                    in_state(Screen::Gameplay)
                        .and(is_multiplayer_mode)
                        .and(not(resource_exists::<SpacetimeDbConnection>)),
                ),
            )
            .add_systems(
                OnExit(Screen::Gameplay),
                disconnect_from_spacetimedb.run_if(is_multiplayer_mode),
            );

        app.add_observer(combat::send_attack_to_server)
            .add_systems(
                Update,
                (
                    reap_dead_connections.run_if(resource_exists::<SpacetimeDbConnection>),
                    handle_connection_events.run_if(resource_exists::<SpacetimeDbConnection>),
                    player::spawn_remote_players.run_if(resource_exists::<SpacetimeDbConnection>),
                    player::buffer_inbound_updates.run_if(resource_exists::<SpacetimeDbConnection>),
                    player::update_remote_players.run_if(resource_exists::<SpacetimeDbConnection>),
                    player::despawn_remote_players.run_if(resource_exists::<SpacetimeDbConnection>),
                    player::interpolate_positions.run_if(resource_exists::<SpacetimeDbConnection>),
                    player::setup_remote_animations
                        .run_if(resource_exists::<SpacetimeDbConnection>),
                    player::animate_remote_players.run_if(resource_exists::<SpacetimeDbConnection>),
                    process_outbound_lag.run_if(resource_exists::<SpacetimeDbConnection>),
                    player::send_local_position.run_if(resource_exists::<SpacetimeDbConnection>),
                    combat::sync_remote_health.run_if(resource_exists::<SpacetimeDbConnection>),
                    combat::sync_local_health.run_if(resource_exists::<SpacetimeDbConnection>),
                    combat::handle_remote_death.run_if(resource_exists::<SpacetimeDbConnection>),
                    combat::request_respawn_on_death
                        .run_if(resource_exists::<SpacetimeDbConnection>),
                    combat::sync_npc_enemies.run_if(resource_exists::<SpacetimeDbConnection>),
                    combat::process_remote_combat_events
                        .run_if(resource_exists::<SpacetimeDbConnection>),
                    measure_ping.run_if(resource_exists::<SpacetimeDbConnection>),
                ),
            );
    }
}

/// Timer for position sync rate limiting
#[derive(Resource)]
pub struct PositionSyncTimer {
    pub timer: Timer,
}

impl Default for PositionSyncTimer {
    fn default() -> Self {
        Self {
            // Send position updates 20 times per second
            timer: Timer::from_seconds(0.05, TimerMode::Repeating),
        }
    }
}

/// Lag simulator for testing network conditions
#[derive(Resource, Clone, Debug)]
pub struct LagSimulator {
    /// Delay in milliseconds for outgoing messages (client -> server)
    pub outbound_delay_ms: u64,
    /// Delay in milliseconds for incoming messages (server -> client)
    pub inbound_delay_ms: u64,
    /// Chance to drop a packet (0.0 - 1.0)
    pub packet_loss_chance: f32,
}

impl Default for LagSimulator {
    fn default() -> Self {
        Self {
            outbound_delay_ms: 0,
            inbound_delay_ms: 0,
            packet_loss_chance: 0.0,
        }
    }
}

/// A pending outbound update with its scheduled send time
#[derive(Clone, Debug)]
struct PendingOutboundUpdate {
    x: f32,
    y: f32,
    z: f32,
    rot_y: f32,
    anim_state: String,
    attack_seq: u32,
    attack_anim: String,
    send_at: Instant,
}

/// Buffered inbound player state with its receive time
#[derive(Clone, Debug)]
struct BufferedInboundState {
    x: f32,
    y: f32,
    z: f32,
    rot_y: f32,
    received_at: Instant,
}

/// Container for delayed network messages
#[derive(Resource, Default)]
pub struct LagBuffers {
    outbound_queue: Vec<PendingOutboundUpdate>,
    inbound_buffer: HashMap<spacetimedb_sdk::Identity, BufferedInboundState>,
}

/// Builds the configured connection builder with all callbacks registered.
macro_rules! connection_builder {
    ($config:expr, $token:expr) => {{
        let token_store = $token.clone();
        let stored = $token.lock().unwrap().clone();
        DbConnection::builder()
            .with_uri(&$config.uri)
            .with_module_name(&$config.module_name)
            .with_token(stored)
            .on_connect(move |conn, identity, token| {
                info!("Connected to SpacetimeDB with identity: {:?}", identity);
                *token_store.lock().unwrap() = Some(token.to_string());
                if let Err(e) = conn.reducers.join_game(Some("Player".to_string())) {
                    error!("Failed to call join_game: {:?}", e);
                }
                conn.subscription_builder().subscribe([
                    "SELECT * FROM player",
                    "SELECT * FROM npc_enemy",
                    "SELECT * FROM combat_event",
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

/// Attempt a SpacetimeDB connection. Returns the resource on success.
/// Reuses a stored token if available so the server recognizes the same player.
pub fn try_connect(
    config: &SpacetimeDbConfig,
    token: &SpacetimeDbToken,
) -> Option<SpacetimeDbConnection> {
    info!("Connecting to SpacetimeDB at {}...", config.uri);
    match connection_builder!(config, token.0).build() {
        Ok(conn) => {
            info!("Connected to SpacetimeDB server");
            Some(SpacetimeDbConnection { conn })
        }
        Err(e) => {
            warn!("SpacetimeDB connection failed: {e:?}");
            None
        }
    }
}

/// Disconnect and remove the connection resource when leaving gameplay.
fn disconnect_from_spacetimedb(
    conn: Option<Res<SpacetimeDbConnection>>,
    mut commands: Commands,
    mut ping: ResMut<PingTracker>,
) {
    if let Some(conn) = conn {
        if let Err(e) = conn.conn.disconnect() {
            warn!("SpacetimeDB disconnect error: {e:?}");
        }
        commands.remove_resource::<SpacetimeDbConnection>();
    }
    *ping = PingTracker::default();
    commands.insert_resource(GameMode::default());
}

/// Periodically attempt to (re)connect when in multiplayer gameplay without a connection.
fn auto_connect(
    config: Res<SpacetimeDbConfig>,
    token: Res<SpacetimeDbToken>,
    mut timer: ResMut<ReconnectTimer>,
    time: Res<Time>,
    mut commands: Commands,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }
    if let Some(conn) = try_connect(&config, &token) {
        commands.insert_resource(conn);
        commands.insert_resource(HandshakeStart(Instant::now()));
        info!("Auto-connect succeeded");
    }
}

/// When `build()` was called for the current connection (WASM build is non-blocking).
#[derive(Resource)]
struct HandshakeStart(Instant);

/// Drop connections that are dead or stuck in handshake.
/// On WASM, `build()` returns Ok immediately — the handshake may never complete.
/// Without this, `auto_connect` never retries because the resource exists.
fn reap_dead_connections(
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

    // Handshake still pending — check timeout
    if let Some(start) = start {
        if start.0.elapsed().as_secs_f32() > HANDSHAKE_TIMEOUT_SECS {
            warn!("Handshake timeout — dropping stale connection for retry");
            let _ = conn.conn.disconnect();
            commands.remove_resource::<SpacetimeDbConnection>();
            commands.remove_resource::<HandshakeStart>();
        }
    }
}


/// Process connection events each frame
fn handle_connection_events(conn: Res<SpacetimeDbConnection>) {
    if let Err(e) = conn.conn.frame_tick() {
        warn!("frame_tick error: {e:?}");
    }
}

/// Measure ping by detecting when our player row's `last_update` changes.
fn measure_ping(conn: Res<SpacetimeDbConnection>, mut tracker: ResMut<PingTracker>) {
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
            // EMA smoothing (alpha = 0.2)
            if tracker.smoothed_rtt_ms <= 0.0 {
                tracker.smoothed_rtt_ms = rtt_ms;
            } else {
                tracker.smoothed_rtt_ms = tracker.smoothed_rtt_ms * 0.8 + rtt_ms * 0.2;
            }
        }
    }
}

/// Process delayed outbound messages
fn process_outbound_lag(
    conn: Res<SpacetimeDbConnection>,
    lag: Res<LagSimulator>,
    mut buffers: ResMut<LagBuffers>,
) {
    if lag.outbound_delay_ms == 0 && lag.packet_loss_chance == 0.0 {
        // No lag simulation, send everything immediately
        for update in buffers.outbound_queue.drain(..) {
            if let Err(e) = conn.conn.reducers.update_position(
                update.x,
                update.y,
                update.z,
                update.rot_y,
                update.anim_state,
                update.attack_seq,
                update.attack_anim,
            ) {
                warn!("Failed to send position update: {:?}", e);
            }
        }
        return;
    }

    let now = Instant::now();
    buffers.outbound_queue.retain(|update| {
        if now >= update.send_at {
            // Check for packet loss simulation
            if lag.packet_loss_chance > 0.0 && rand::random::<f32>() < lag.packet_loss_chance {
                info!("Simulating packet loss for outbound update");
                return false; // Drop the packet
            }

            if let Err(e) = conn.conn.reducers.update_position(
                update.x,
                update.y,
                update.z,
                update.rot_y,
                update.anim_state.clone(),
                update.attack_seq,
                update.attack_anim.clone(),
            ) {
                warn!("Failed to send position update: {:?}", e);
            }
            false // Remove from queue
        } else {
            true // Keep in queue
        }
    });
}

//! SpacetimeDB multiplayer networking module

use bevy::prelude::*;
use spacetimedb_sdk::DbContext;

pub mod combat;
pub mod generated;
pub mod player;

pub use generated::{DbConnection, Player, Reducer};
pub use player::*;

use generated::join_game_reducer::join_game;
use generated::leave_game_reducer::leave_game;
use generated::update_position_reducer::update_position;
use std::collections::HashMap;
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

/// Plugin for SpacetimeDB networking
pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpacetimeDbConfig>()
            .init_resource::<PositionSyncTimer>()
            .init_resource::<LagSimulator>()
            .init_resource::<LagBuffers>()
            .init_resource::<combat::CombatEventTracker>()
            .add_systems(PostStartup, connect_to_spacetimedb);

        app.add_observer(combat::send_attack_to_server)
            .add_systems(
                Update,
                (
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
    ($config:expr) => {
        DbConnection::builder()
            .with_uri(&$config.uri)
            .with_module_name(&$config.module_name)
            .on_connect(|conn, identity, _token| {
                info!("Connected to SpacetimeDB with identity: {:?}", identity);
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
    };
}

/// Connect to SpacetimeDB on startup
fn connect_to_spacetimedb(config: Res<SpacetimeDbConfig>, mut commands: Commands) {
    info!("Connecting to SpacetimeDB at {}...", config.uri);

    #[cfg(not(target_arch = "wasm32"))]
    {
        match connection_builder!(config).build() {
            Ok(conn) => {
                info!("Connected to SpacetimeDB server");
                commands.insert_resource(SpacetimeDbConnection { conn });
            }
            Err(e) => {
                warn!("No SpacetimeDB server found — running in offline mode: {e:?}");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        info!("WASM: calling build() for SpacetimeDB connection");
        match connection_builder!(config).build() {
            Ok(conn) => {
                // Don't call run_background_task() — it holds mutex locks across await
                // points, which conflicts with the synchronous frame_tick() called by
                // handle_connection_events each frame. frame_tick() alone is sufficient.
                commands.insert_resource(SpacetimeDbConnection { conn });
                info!("WASM: SpacetimeDbConnection resource inserted");
            }
            Err(e) => {
                error!("WASM: build() failed: {e:?}");
            }
        }
    }
}

/// Process connection events each frame
fn handle_connection_events(conn: Res<SpacetimeDbConnection>) {
    if let Err(e) = conn.conn.frame_tick() {
        warn!("frame_tick error: {e:?}");
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

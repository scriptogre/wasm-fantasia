//! SpacetimeDB multiplayer networking module

use bevy::prelude::*;
use spacetimedb_sdk::DbContext;

pub mod generated;
pub mod player;

pub use player::*;
pub use generated::{DbConnection, Player, Reducer};

use generated::join_game_reducer::join_game;
use generated::leave_game_reducer::leave_game;

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
            uri: "ws://127.0.0.1:3000".to_string(),
            module_name: "wasm-fantasia".to_string(),
        }
    }
}

/// Plugin for SpacetimeDB networking
pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpacetimeDbConfig>()
            .init_resource::<PositionSyncTimer>()
            .add_systems(PostStartup, connect_to_spacetimedb)
            .add_systems(Update, (
                handle_connection_events.run_if(resource_exists::<SpacetimeDbConnection>),
                player::spawn_remote_players.run_if(resource_exists::<SpacetimeDbConnection>),
                player::update_remote_players.run_if(resource_exists::<SpacetimeDbConnection>),
                player::despawn_remote_players.run_if(resource_exists::<SpacetimeDbConnection>),
                player::interpolate_positions.run_if(resource_exists::<SpacetimeDbConnection>),
                player::send_local_position.run_if(resource_exists::<SpacetimeDbConnection>),
            ));
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

/// Connect to SpacetimeDB on startup
fn connect_to_spacetimedb(config: Res<SpacetimeDbConfig>, mut commands: Commands) {
    info!("Connecting to SpacetimeDB at {}...", config.uri);

    let conn = DbConnection::builder()
        .with_uri(&config.uri)
        .with_module_name(&config.module_name)
        .on_connect(|conn, identity, _token| {
            info!("Connected to SpacetimeDB with identity: {:?}", identity);
            if let Err(e) = conn.reducers.join_game(Some("Player".to_string())) {
                error!("Failed to call join_game: {:?}", e);
            }
            // Subscribe to all players
            conn.subscription_builder().subscribe(["SELECT * FROM player"]);
        })
        .on_connect_error(|_ctx, err| {
            error!("Failed to connect to SpacetimeDB: {:?}", err);
        })
        .on_disconnect(|ctx, err| {
            warn!("Disconnected from SpacetimeDB: {:?}", err);
            // Try to call leave_game before disconnect completes
            let _ = ctx.reducers.leave_game();
        })
        .build();

    let Ok(conn) = conn else {
        error!("Failed to build SpacetimeDB connection");
        return;
    };

    // Just use frame_tick() in Update instead of background thread
    // conn.run_threaded() was causing window issues on macOS

    commands.insert_resource(SpacetimeDbConnection { conn });
}

/// Process connection events each frame
fn handle_connection_events(conn: Res<SpacetimeDbConnection>) {
    let _ = conn.conn.frame_tick();
}

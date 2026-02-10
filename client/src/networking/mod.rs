//! SpacetimeDB multiplayer networking module

use bevy::prelude::*;
use spacetimedb_sdk::DbContext;

use crate::combat::{AttackState, Combatant, Enemy, EnemyBehavior, Health};
use crate::models::{GameMode, GameplayCleanup, Player as LocalPlayer, Screen, ServerTarget};
use crate::player::Animation;
use crate::rules::{Stat, Stats};
use avian3d::prelude::{Collider, LockedAxes, Mass, RigidBody};
use spacetimedb_sdk::Table;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use wasm_fantasia_shared::combat::EnemyBehaviorKind;
use web_time::Instant;

pub mod combat;
pub mod generated;
#[cfg(not(target_arch = "wasm32"))]
pub mod local_server;

pub use generated::{DbConnection, Player, Reducer};

use generated::combat_event_table::CombatEventTableAccess;
use generated::enemy_table::EnemyTableAccess;
use generated::join_game_reducer::join_game;
use generated::leave_game_reducer::leave_game;
use generated::player_table::PlayerTableAccess;
use generated::update_position_reducer::update_position;

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

/// Persists the SpacetimeDB auth token across reconnects.
#[derive(Resource, Default, Clone)]
pub struct SpacetimeDbToken(pub Arc<Mutex<Option<String>>>);

/// Tracks round-trip time by comparing position send timestamps against server acks.
#[derive(Resource, Default)]
pub struct PingTracker {
    pub last_send: Option<Instant>,
    pub last_seen_update: i64,
    pub smoothed_rtt_ms: f32,
    pub last_ack: Option<Instant>,
}

/// Run condition: true when a SpacetimeDB connection is live (any mode).
pub fn is_server_connected(conn: Option<Res<SpacetimeDbConnection>>) -> bool {
    conn.is_some()
}

pub const STALE_THRESHOLD_SECS: f32 = 3.0;
const HANDSHAKE_TIMEOUT_SECS: f32 = 5.0;
const RECONNECT_INTERVAL_SECS: f32 = 2.0;
const INTERPOLATION_SPEED: f32 = 12.0;

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

/// Tracks which CombatEvent IDs have been processed.
#[derive(Resource, Default)]
pub struct CombatEventTracker {
    last_processed_id: u64,
}

// =============================================================================
// Reconciler components
// =============================================================================

/// Links an ECS entity to a server table row.
#[derive(Component, Clone, Hash, Eq, PartialEq, Debug)]
pub enum ServerId {
    Player(spacetimedb_sdk::Identity),
    Enemy(u64),
}

/// Target position for interpolation. Written by reconciler, consumed by interpolation system.
#[derive(Component, Clone, Debug)]
pub struct WorldEntity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,
}

/// Offensive combat stats synced from server.
#[derive(Component, Clone, Debug)]
pub struct CombatStats {
    pub attack_damage: f32,
    pub crit_chance: f32,
    pub crit_multiplier: f32,
    pub attack_range: f32,
    pub attack_arc: f32,
    pub knockback_force: f32,
    pub attack_speed: f32,
    pub last_attack_time: i64,
}

/// Marker for data carried by combat event entities.
#[derive(Component)]
pub struct CombatEventData {
    pub damage: f32,
    pub is_crit: bool,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

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
            .init_resource::<ReconnectTimer>()
            .init_resource::<PositionSyncTimer>()
            .init_resource::<PingTracker>()
            .init_resource::<CombatEventTracker>()
            .add_systems(
                OnEnter(Screen::Connecting),
                reset_reconnect_timer.run_if(resource_exists::<ServerTarget>),
            )
            .add_systems(Update, auto_connect)
            .add_systems(OnExit(Screen::Connecting), cleanup_connecting_exit)
            .add_systems(
                OnExit(Screen::Gameplay),
                (disconnect_from_spacetimedb, remove_server_target)
                    .run_if(is_server_connected)
                    .before(GameplayCleanup),
            );

        app.add_observer(combat::send_attack_to_server).add_systems(
            Update,
            (
                reap_dead_connections.run_if(resource_exists::<SpacetimeDbConnection>),
                handle_connection_events.run_if(resource_exists::<SpacetimeDbConnection>),
                reconcile.run_if(resource_exists::<SpacetimeDbConnection>),
                interpolate_synced_entities.run_if(resource_exists::<SpacetimeDbConnection>),
                send_local_position.run_if(resource_exists::<SpacetimeDbConnection>),
                combat::request_respawn_on_death.run_if(resource_exists::<SpacetimeDbConnection>),
                measure_ping.run_if(resource_exists::<SpacetimeDbConnection>),
            ),
        );
    }
}

// =============================================================================
// Connection lifecycle
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

fn reset_reconnect_timer(mut timer: ResMut<ReconnectTimer>) {
    *timer = ReconnectTimer::default();
}

/// Clean up when leaving the Connecting screen without a completed handshake.
fn cleanup_connecting_exit(conn: Option<Res<SpacetimeDbConnection>>, mut commands: Commands) {
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

    // Remove local server so stale LocalServerState::Failed doesn't block
    // the next auto_connect attempt.
    #[cfg(not(target_arch = "wasm32"))]
    {
        commands.remove_resource::<local_server::LocalServer>();
        commands.remove_resource::<local_server::LocalServerState>();
    }
}

fn disconnect_from_spacetimedb(
    conn: Option<Res<SpacetimeDbConnection>>,
    mut commands: Commands,
    mut ping: ResMut<PingTracker>,
    mut mode: ResMut<GameMode>,
) {
    if let Some(conn) = conn {
        if let Err(e) = conn.conn.disconnect() {
            warn!("SpacetimeDB disconnect error: {e:?}");
        }
        commands.remove_resource::<SpacetimeDbConnection>();
    }
    *ping = PingTracker::default();
    *mode = GameMode::default();
}

fn remove_server_target(mut commands: Commands) {
    commands.remove_resource::<ServerTarget>();
}

fn auto_connect(
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
        if **ls_state != local_server::LocalServerState::Ready {
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

#[derive(Resource)]
struct HandshakeStart(Instant);

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

    if let Some(start) = start {
        if start.0.elapsed().as_secs_f32() > HANDSHAKE_TIMEOUT_SECS {
            warn!("Handshake timeout — dropping stale connection for retry");
            let _ = conn.conn.disconnect();
            commands.remove_resource::<SpacetimeDbConnection>();
            commands.remove_resource::<HandshakeStart>();
        }
    }
}

fn handle_connection_events(conn: Res<SpacetimeDbConnection>) {
    if let Err(e) = conn.conn.frame_tick() {
        warn!("frame_tick error: {e:?}");
    }
}

// =============================================================================
// The Reconciler
// =============================================================================

/// One system that diffs the SpacetimeDB client cache against the ECS each frame.
/// Spawns, patches, or despawns entities to match server state.
fn reconcile(
    conn: Res<SpacetimeDbConnection>,
    mut remote_entities: Query<
        (
            Entity,
            &ServerId,
            &mut WorldEntity,
            &mut Health,
            Option<&mut EnemyBehavior>,
        ),
        Without<LocalPlayer>,
    >,
    mut local_health: Query<(&mut Health, &mut Stats), With<LocalPlayer>>,
    mut tracker: ResMut<CombatEventTracker>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let my_id = conn.conn.try_identity();
    let mut seen = HashSet::new();

    // ── Collect all entity tables into one flat list ───
    struct Row {
        id: ServerId,
        world: WorldEntity,
        health: f32,
        max_health: f32,
        animation_state: String,
    }

    let rows: Vec<Row> = conn
        .conn
        .db
        .player()
        .iter()
        .filter(|p| p.online)
        .map(|p| Row {
            id: ServerId::Player(p.identity),
            world: WorldEntity {
                x: p.x,
                y: p.y,
                z: p.z,
                rotation_y: p.rotation_y,
            },
            health: p.health,
            max_health: p.max_health,
            animation_state: String::new(),
        })
        .chain(conn.conn.db.enemy().iter().map(|e| Row {
            id: ServerId::Enemy(e.id),
            world: WorldEntity {
                x: e.x,
                y: e.y,
                z: e.z,
                rotation_y: e.rotation_y,
            },
            health: e.health,
            max_health: e.max_health,
            animation_state: e.animation_state.clone(),
        }))
        .collect();

    // ── Local player: patch health, skip spawning ─────
    for row in &rows {
        if let ServerId::Player(identity) = &row.id {
            if Some(*identity) == my_id {
                if let Ok((mut health, mut stats)) = local_health.single_mut() {
                    health.current = row.health;
                    health.max = row.max_health;
                    stats.set(Stat::Health, row.health);
                    stats.set(Stat::MaxHealth, row.max_health);
                }
                seen.insert(row.id.clone());
            }
        }
    }

    // ── Patch or despawn existing remote entities ──────
    for (bevy_entity, id, mut world_entity, mut health, enemy_behavior) in &mut remote_entities {
        if let Some(row) = rows.iter().find(|r| &r.id == id) {
            seen.insert(id.clone());
            *world_entity = row.world.clone();
            health.current = row.health;
            health.max = row.max_health;

            // Patch enemy behavior from server animation_state
            if let Some(mut behavior) = enemy_behavior {
                let kind = EnemyBehaviorKind::parse_str(&row.animation_state);
                let new_behavior = match kind {
                    EnemyBehaviorKind::Idle => EnemyBehavior::Idle,
                    EnemyBehaviorKind::Chase => EnemyBehavior::Chase,
                    EnemyBehaviorKind::Attack => EnemyBehavior::Attack,
                };
                if *behavior != new_behavior {
                    *behavior = new_behavior;
                }
            }
        } else {
            commands.entity(bevy_entity).despawn();
        }
    }

    // ── Spawn new remote entities ──────────────────────
    for row in &rows {
        if seen.contains(&row.id) {
            continue;
        }

        let is_enemy = matches!(&row.id, ServerId::Enemy(_));
        let name = match &row.id {
            ServerId::Player(id) => format!("RemotePlayer_{id:?}"),
            ServerId::Enemy(id) => format!("Enemy_{id}"),
        };

        if is_enemy {
            // Enemy: On<Add, Enemy> observer attaches GLTF model + animations
            commands.spawn((
                Name::new(name),
                row.id.clone(),
                row.world.clone(),
                Transform::from_xyz(row.world.x, row.world.y, row.world.z),
                Health::new(row.max_health),
                Enemy,
                Combatant,
                Stats::new()
                    .with(Stat::MaxHealth, row.max_health)
                    .with(Stat::Health, row.health),
            ));
        } else {
            // Remote player: capsule mesh (TODO: player model for remotes)
            let material = materials.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.6, 1.0),
                ..default()
            });
            let mesh = meshes.add(Capsule3d::new(0.5, 1.0));

            commands.spawn((
                Name::new(name),
                row.id.clone(),
                row.world.clone(),
                Transform::from_xyz(row.world.x, row.world.y, row.world.z),
                Health::new(row.max_health),
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Collider::capsule(0.5, 1.0),
                RigidBody::Dynamic,
                LockedAxes::ROTATION_LOCKED,
                Mass(500.0),
            ));
        }
    }

    // ── Combat events ─────────────────────────────────
    for event in conn.conn.db.combat_event().iter() {
        if event.id <= tracker.last_processed_id {
            continue;
        }
        tracker.last_processed_id = event.id;

        commands.spawn((
            CombatEventData {
                damage: event.damage,
                is_crit: event.is_crit,
                x: event.x,
                y: event.y,
                z: event.z,
            },
            Transform::from_xyz(event.x, event.y, event.z),
        ));
    }
}

// =============================================================================
// Interpolation
// =============================================================================

/// Smoothly move `Transform` toward the server-authoritative `WorldEntity` target.
fn interpolate_synced_entities(
    time: Res<Time>,
    mut query: Query<(&WorldEntity, &mut Transform), With<ServerId>>,
) {
    let alpha = (time.delta_secs() * INTERPOLATION_SPEED).min(1.0);
    for (world_entity, mut transform) in &mut query {
        let target = Vec3::new(world_entity.x, world_entity.y, world_entity.z);
        transform.translation = transform.translation.lerp(target, alpha);
        transform.rotation = Quat::slerp(
            transform.rotation,
            Quat::from_rotation_y(world_entity.rotation_y),
            alpha,
        );
    }
}

// =============================================================================
// Outbound position relay
// =============================================================================

/// Send local player position to the server at a fixed rate.
fn send_local_position(
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

// =============================================================================
// Ping measurement
// =============================================================================

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
            if tracker.smoothed_rtt_ms <= 0.0 {
                tracker.smoothed_rtt_ms = rtt_ms;
            } else {
                tracker.smoothed_rtt_ms = tracker.smoothed_rtt_ms * 0.8 + rtt_ms * 0.2;
            }
        }
    }
}

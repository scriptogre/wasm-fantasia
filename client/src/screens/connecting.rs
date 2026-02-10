//! Dedicated connecting screen — sits between Title and Gameplay.
//! Shows a console-style connection log with live status updates.
//!
//! Handles both local server startup (native SP) and remote connections (MP / web solo).

use super::*;

use crate::networking::{ReconnectTimer, SpacetimeDbConfig, SpacetimeDbConnection};
use crate::ui::hud::HudFont;
use spacetimedb_sdk::DbContext;

const CONNECTION_TIMEOUT_SECS: f32 = 10.0;

// ── Resources ───────────────────────────────────────────────────────

#[derive(Resource)]
struct ConnectionTimeout(Timer);

#[derive(Resource, Default)]
struct ConnectionLog {
    lines: Vec<String>,
    showed_target: bool,
    saw_resource: bool,
    saw_identity: bool,
}

impl ConnectionLog {
    fn push(&mut self, line: impl Into<String>) {
        let line = line.into();
        info!("[connect] {line}");
        self.lines.push(line);
    }

    fn display(&self) -> String {
        self.lines.join("\n")
    }
}

// ── Components ──────────────────────────────────────────────────────

#[derive(Component)]
struct LogText;

// ── Plugin ──────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Connecting), spawn_connecting_screen)
        .add_systems(
            Update,
            (
                advance_local_server,
                track_connection_state,
                tick_connection,
                tick_timeout,
                update_log_display,
            )
                .chain()
                .run_if(in_state(Screen::Connecting)),
        );
}

// ── Spawn ───────────────────────────────────────────────────────────

fn spawn_connecting_screen(
    mut commands: Commands,
    font: Res<HudFont>,
    server_target: Option<Res<ServerTarget>>,
    config: Res<SpacetimeDbConfig>,
) {
    info!("Entering connecting screen");

    let mut log = ConnectionLog::default();

    match server_target.as_deref() {
        Some(ServerTarget::Local { .. }) => {
            log.push("Starting local SpacetimeDB server...");
        }
        Some(ServerTarget::Remote { uri }) => {
            log.push(format!("Connecting to {} ({})...", uri, config.module_name));
        }
        None => {
            log.push(format!(
                "Connecting to {} ({})...",
                config.uri, config.module_name
            ));
        }
    }

    commands.insert_resource(log);
    commands.insert_resource(ConnectionTimeout(Timer::from_seconds(
        CONNECTION_TIMEOUT_SECS,
        TimerMode::Once,
    )));

    let log_font = TextFont {
        font: font.0.clone(),
        font_size: 14.0,
        ..default()
    };

    commands
        .spawn((
            DespawnOnExit(Screen::Connecting),
            GlobalZIndex(1),
            ui_root("Connecting Screen"),
            BackgroundColor(colors::NEUTRAL950.with_alpha(0.95)),
        ))
        .with_children(|root| {
            // Title
            root.spawn((
                Text::new("CONNECTING"),
                TextFont {
                    font: font.0.clone(),
                    font_size: 24.0,
                    ..default()
                },
                TextColor(colors::NEUTRAL300),
            ));

            // Console log area
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    padding: UiRect::all(Px(16.0)),
                    min_width: Vw(50.0),
                    min_height: Vh(15.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3)),
                BorderRadius::all(Px(4.0)),
            ))
            .with_children(|log_area| {
                log_area.spawn((
                    LogText,
                    Text::new(""),
                    log_font,
                    TextColor(Color::srgb(0.4, 0.8, 0.4)),
                ));
            });

            root.spawn(btn_small("Cancel", cancel_connecting));
        });
}

fn cancel_connecting(_: On<Pointer<Click>>, mut commands: Commands) {
    commands.trigger(GoTo(Screen::Title));
}

// ── Local server state machine ──────────────────────────────────────

/// Drive the local SpacetimeDB subprocess forward (native SP only).
/// Once the server is ready, inserts the reconnect timer so `auto_connect` fires.
#[allow(unused_variables, unused_mut)]
fn advance_local_server(
    mut log: ResMut<ConnectionLog>,
    mut commands: Commands,
    #[cfg(not(target_arch = "wasm32"))] mut server: Option<
        ResMut<crate::networking::local_server::LocalServer>,
    >,
    #[cfg(not(target_arch = "wasm32"))] mut server_state: Option<
        ResMut<crate::networking::local_server::LocalServerState>,
    >,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::networking::local_server::{self, LocalServerState};

        let (Some(ref mut server), Some(ref mut state)) = (server, server_state) else {
            return;
        };

        let changed = local_server::advance(server, state);
        if !changed {
            return;
        }

        match state.as_ref() {
            LocalServerState::WaitingForReady => {
                log.push("Waiting for server to start...");
            }
            LocalServerState::Deploying(_) => {
                log.push("Server started. Deploying game module...");
            }
            LocalServerState::Ready => {
                let uri = local_server::connection_uri(server);
                log.push(format!("Module deployed. Connecting to {uri}..."));
                // Kick off the reconnect timer so auto_connect fires
                commands.insert_resource(ReconnectTimer::default());
            }
            LocalServerState::Failed(err) => {
                log.push(format!("Local server error: {err}"));
                // Go back to title after a brief pause
                commands.trigger(GoTo(Screen::Title));
            }
            LocalServerState::Starting => {}
        }
    }
}

// ── Connection state tracking ───────────────────────────────────────

fn track_connection_state(
    conn: Option<Res<SpacetimeDbConnection>>,
    mode: Res<GameMode>,
    timer: Res<ReconnectTimer>,
    screen: Res<State<Screen>>,
    mut log: ResMut<ConnectionLog>,
) {
    // First run: dump connection diagnostics
    if !log.showed_target {
        log.showed_target = true;
        let has_conn = conn.is_some();
        log.push(format!(
            "state={:?} mode={:?} conn={} timer={:.2}/{:.2}",
            screen.get(),
            *mode,
            has_conn,
            timer.0.elapsed_secs(),
            timer.0.duration().as_secs_f32(),
        ));
        if has_conn {
            log.push("WARNING: stale SpacetimeDbConnection resource exists");
        }
    }

    // Connection resource appeared — server responded
    if conn.is_some() && !log.saw_resource {
        log.saw_resource = true;
        log.push("Server responded. Completing handshake...");
    }

    // Identity available — handshake done
    if let Some(conn) = &conn {
        if conn.conn.try_identity().is_some() && !log.saw_identity {
            log.saw_identity = true;
            log.push("Handshake complete!");
            log.push("Entering gameplay...");
        }
    }

    // Connection dropped after being established — retry cycle
    if conn.is_none() && log.saw_resource {
        log.saw_resource = false;
        log.saw_identity = false;
        log.push("Connection lost. Retrying...");
    }
}

// ── Log display sync ────────────────────────────────────────────────

fn update_log_display(log: Res<ConnectionLog>, mut text: Query<&mut Text, With<LogText>>) {
    if !log.is_changed() {
        return;
    }
    if let Ok(mut text) = text.single_mut() {
        text.0 = log.display();
    }
}

// ── Connection check + timeout ──────────────────────────────────────

fn tick_connection(
    connection: Option<Res<SpacetimeDbConnection>>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    let connected = connection
        .as_ref()
        .is_some_and(|c| c.conn.try_identity().is_some());
    if connected {
        next_screen.set(Screen::Gameplay);
    }
}

fn tick_timeout(
    mut timeout: ResMut<ConnectionTimeout>,
    mut log: ResMut<ConnectionLog>,
    time: Res<Time>,
    mut commands: Commands,
) {
    timeout.0.tick(time.delta());
    if timeout.0.just_finished() {
        log.push("Connection timed out.");
        commands.trigger(GoTo(Screen::Title));
    }
}

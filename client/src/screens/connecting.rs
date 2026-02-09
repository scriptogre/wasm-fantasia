//! Dedicated connecting screen for multiplayer — sits between Title and Gameplay.
//! Shows a console-style connection log with live status updates.

use super::*;

use crate::networking::{ReconnectTimer, SpacetimeDbConfig, SpacetimeDbConnection};
use crate::ui::hud::HudFont;
use spacetimedb_sdk::DbContext;

const CONNECTION_TIMEOUT_SECS: f32 = 10.0;

// ── Resources ───────────────────────────────────────────────────────

#[derive(Resource)]
struct ConnectionTimeout(Timer);

#[derive(Resource)]
struct ConnectionLog {
    lines: Vec<String>,
    showed_target: bool,
    saw_resource: bool,
    saw_identity: bool,
}

impl Default for ConnectionLog {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            showed_target: false,
            saw_resource: false,
            saw_identity: false,
        }
    }
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
                track_connection_state,
                tick_connection,
                tick_timeout,
                update_log_display,
            )
                .run_if(in_state(Screen::Connecting)),
        );
}

// ── Spawn ───────────────────────────────────────────────────────────

fn spawn_connecting_screen(
    mut commands: Commands,
    font: Res<HudFont>,
    config: Res<SpacetimeDbConfig>,
) {
    info!("Entering connecting screen");

    let mut log = ConnectionLog::default();
    log.push(format!(
        "Connecting to {} ({})...",
        config.uri, config.module_name
    ));

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

// ── Connection state tracking ───────────────────────────────────────

fn track_connection_state(
    conn: Option<Res<SpacetimeDbConnection>>,
    mode: Res<GameMode>,
    timer: Res<ReconnectTimer>,
    screen: Res<State<Screen>>,
    mut log: ResMut<ConnectionLog>,
) {
    // First run: dump everything needs_connection checks
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
        if *mode != GameMode::Multiplayer {
            log.push(format!(
                "WARNING: GameMode is {:?}, expected Multiplayer",
                *mode
            ));
        }
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

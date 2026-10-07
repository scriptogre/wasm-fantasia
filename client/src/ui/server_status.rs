//! Multiplayer status line under the health bar: connection, players online, ping

use bevy::prelude::*;
use spacetimedb_sdk::{DbContext, Table};

use crate::models::{Screen, is_multiplayer_mode};
use crate::networking::generated::player_table::PlayerTableAccess;
use crate::networking::{PingTracker, STALE_THRESHOLD_SECS, SpacetimeDbConnection};
use crate::ui::colors::{ALERT, AMBER, INK};
use crate::ui::hud::HUD_SHADOW;
use crate::ui::size::{CAPTION_SIZE, EDGE};

const OFFLINE: &str = "Offline";
const NO_PLAYERS: &str = "";
const NO_PING: &str = "";
// ── Components ──────────────────────────────────────────────────────

#[derive(Component)]
struct StatusDot;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct PlayersText;

#[derive(Component)]
struct PingText;

// ── Plugin ──────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_systems(
        OnEnter(Screen::Gameplay),
        spawn_status_hud.run_if(is_multiplayer_mode),
    )
    .add_systems(
        Update,
        (tick_status, tick_players, tick_ping)
            .run_if(in_state(Screen::Gameplay).and(is_multiplayer_mode)),
    );
}

// ── Spawn ───────────────────────────────────────────────────────────

/// One row under the health bar: square marker, status, players, ping.
fn spawn_status_hud(mut commands: Commands) {
    let text = || {
        (
            TextFont::from_font_size(CAPTION_SIZE),
            TextColor(INK),
            HUD_SHADOW,
        )
    };

    commands.spawn((
        Name::new("Server Status"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(EDGE + 26.0),
            left: Val::Px(EDGE),
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            ..default()
        },
        GlobalZIndex(90),
        Pickable::IGNORE,
        children![
            (
                StatusDot,
                Node {
                    width: Val::Px(8.0),
                    height: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(ALERT),
            ),
            (StatusText, Text::new(OFFLINE), text()),
            (PlayersText, Text::new(NO_PLAYERS), text()),
            (PingText, Text::new(NO_PING), text()),
        ],
    ));
}

// ── Tick systems ────────────────────────────────────────────────────

/// Derive connection status from three independent signals:
/// 1. is_active() — send channel exists (can go stale if server crashes)
/// 2. try_identity() — handshake completed (None on WASM before on_connect)
/// 3. last_ack — server actually responded recently (catches silent deaths)
fn connection_status(
    conn: &Option<Res<SpacetimeDbConnection>>,
    tracker: &Option<Res<PingTracker>>,
) -> (&'static str, Color) {
    let Some(conn) = conn.as_ref() else {
        return (OFFLINE, ALERT);
    };
    if !conn.conn.is_active() || conn.conn.try_identity().is_none() {
        return (OFFLINE, ALERT);
    }
    // Connection looks alive — check if server is actually responding
    if let Some(tracker) = tracker.as_ref() {
        if let Some(last_ack) = tracker.last_ack {
            if last_ack.elapsed().as_secs_f32() > STALE_THRESHOLD_SECS {
                return ("Unstable", AMBER);
            }
        }
    }
    ("Online", INK)
}

fn tick_status(
    conn: Option<Res<SpacetimeDbConnection>>,
    tracker: Option<Res<PingTracker>>,
    mut dots: Query<&mut BackgroundColor, With<StatusDot>>,
    mut texts: Query<&mut Text, With<StatusText>>,
) {
    let (label, color) = connection_status(&conn, &tracker);

    if let Ok(mut dot) = dots.single_mut() {
        dot.0 = color;
    }
    if let Ok(mut text) = texts.single_mut() {
        if text.0 != label {
            text.0 = label.to_string();
        }
    }
}

fn tick_players(
    conn: Option<Res<SpacetimeDbConnection>>,
    tracker: Option<Res<PingTracker>>,
    mut texts: Query<&mut Text, With<PlayersText>>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    let Ok(mut text) = texts.single_mut() else {
        return;
    };

    let (label, _) = connection_status(&conn, &tracker);
    if label == OFFLINE {
        if text.0 != NO_PLAYERS {
            text.0 = NO_PLAYERS.to_string();
        }
        return;
    }

    // Throttle to twice per second — player count changes rarely
    *timer += time.delta_secs();
    if *timer < 0.5 {
        return;
    }
    *timer = 0.0;

    let online = conn.as_ref().map_or(0, |c| {
        c.conn.db.player().iter().filter(|p| p.online).count()
    });
    let new = match online {
        1 => "1 player".to_string(),
        n => format!("{n} players"),
    };
    if text.0 != new {
        text.0 = new;
    }
}

fn tick_ping(
    conn: Option<Res<SpacetimeDbConnection>>,
    tracker: Option<Res<PingTracker>>,
    mut texts: Query<&mut Text, With<PingText>>,
    mut colors: Query<&mut TextColor, With<PingText>>,
) {
    let Ok(mut text) = texts.single_mut() else {
        return;
    };

    let (label, _) = connection_status(&conn, &tracker);
    if label == OFFLINE {
        if text.0 != NO_PING {
            text.0 = NO_PING.to_string();
        }
        if let Ok(mut tc) = colors.single_mut() {
            tc.0 = INK;
        }
        return;
    }

    let ms = tracker.as_ref().map(|t| t.smoothed_rtt_ms).unwrap_or(0.0);
    let new = if ms > 0.0 {
        format!("{ms:.0} ms")
    } else {
        NO_PING.to_string()
    };
    if text.0 != new {
        text.0 = new;
    }

    let color = if ms >= 150.0 {
        ALERT
    } else if ms >= 80.0 {
        AMBER
    } else {
        INK
    };
    if let Ok(mut tc) = colors.single_mut() {
        tc.0 = color;
    }
}

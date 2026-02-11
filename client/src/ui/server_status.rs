//! Multiplayer status HUD — connection state, player count, ping

use bevy::prelude::*;
use spacetimedb_sdk::{DbContext, Table};

use crate::models::{Screen, is_multiplayer_mode};
use crate::networking::generated::player_table::PlayerTableAccess;
use crate::networking::{PingTracker, STALE_THRESHOLD_SECS, SpacetimeDbConnection};
use crate::ui::colors::NEUTRAL300;
use crate::ui::hud::HudFont;

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

// ── Colors ──────────────────────────────────────────────────────────

const GREEN: Color = Color::srgb(0.286, 0.878, 0.373);
const RED: Color = Color::srgb(0.816, 0.125, 0.125);
const YELLOW: Color = Color::srgb(0.878, 0.780, 0.286);

// ── Spawn ───────────────────────────────────────────────────────────

fn spawn_status_hud(mut commands: Commands, font: Res<HudFont>) {
    let font = font.0.clone();
    let text_style = TextFont {
        font: font.clone(),
        font_size: 14.0,
        ..default()
    };

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(32.0),
                right: Val::Px(32.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(4.0),
                ..default()
            },
            GlobalZIndex(90),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            // Row 1: status dot + "ONLINE" / "OFFLINE"
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        StatusText,
                        Text::new("OFFLINE"),
                        text_style.clone(),
                        TextColor(NEUTRAL300),
                    ));
                    row.spawn((
                        StatusDot,
                        Node {
                            width: Val::Px(8.0),
                            height: Val::Px(8.0),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(RED),
                    ));
                });

            // Row 2: player count
            parent.spawn((
                PlayersText,
                Text::new("0 / 0"),
                text_style.clone(),
                TextColor(NEUTRAL300),
            ));

            // Row 3: ping
            parent.spawn((
                PingText,
                Text::new("-- ms"),
                text_style,
                TextColor(NEUTRAL300),
            ));
        });
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
        return ("OFFLINE", RED);
    };
    if !conn.conn.is_active() || conn.conn.try_identity().is_none() {
        return ("OFFLINE", RED);
    }
    // Connection looks alive — check if server is actually responding
    if let Some(tracker) = tracker.as_ref() {
        if let Some(last_ack) = tracker.last_ack {
            if last_ack.elapsed().as_secs_f32() > STALE_THRESHOLD_SECS {
                return ("STALE", YELLOW);
            }
        }
    }
    ("ONLINE", GREEN)
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
) {
    let Ok(mut text) = texts.single_mut() else {
        return;
    };

    let (label, _) = connection_status(&conn, &tracker);
    if label == "OFFLINE" {
        if text.0 != "-- / --" {
            text.0 = "-- / --".to_string();
        }
        return;
    }

    let (online, total) = conn
        .as_ref()
        .map(|c| {
            let players: Vec<_> = c.conn.db.player().iter().collect();
            let online = players.iter().filter(|p| p.online).count();
            (online, players.len())
        })
        .unwrap_or((0, 0));

    let new = format!("{online} / {total}");
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
    if label == "OFFLINE" {
        if text.0 != "-- ms" {
            text.0 = "-- ms".to_string();
        }
        if let Ok(mut tc) = colors.single_mut() {
            tc.0 = NEUTRAL300;
        }
        return;
    }

    let ms = tracker.as_ref().map(|t| t.smoothed_rtt_ms).unwrap_or(0.0);
    let new = if ms > 0.0 {
        format!("{ms:.0} ms")
    } else {
        "-- ms".to_string()
    };
    if text.0 != new {
        text.0 = new;
    }

    let color = if ms <= 0.0 {
        NEUTRAL300
    } else if ms < 80.0 {
        GREEN
    } else if ms < 150.0 {
        YELLOW
    } else {
        RED
    };
    if let Ok(mut tc) = colors.single_mut() {
        tc.0 = color;
    }
}

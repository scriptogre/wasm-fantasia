//! Multiplayer status HUD — connection state, player count, ping

use bevy::prelude::*;
use spacetimedb_sdk::{DbContext, Table};

use crate::models::{is_multiplayer_mode, Screen};
use crate::networking::generated::player_table::PlayerTableAccess;
use crate::networking::{PingTracker, SpacetimeDbConnection};
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
            DespawnOnExit(Screen::Gameplay),
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
                            ..default()
                        },
                        BackgroundColor(RED),
                        BorderRadius::all(Val::Px(4.0)),
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

fn tick_status(
    conn: Option<Res<SpacetimeDbConnection>>,
    mut dots: Query<&mut BackgroundColor, With<StatusDot>>,
    mut texts: Query<&mut Text, With<StatusText>>,
) {
    let active = conn
        .as_ref()
        .map(|c| c.conn.is_active())
        .unwrap_or(false);

    let (label, color) = if active {
        ("ONLINE", GREEN)
    } else {
        ("OFFLINE", RED)
    };

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
    mut texts: Query<&mut Text, With<PlayersText>>,
) {
    let (online, total) = conn
        .as_ref()
        .map(|c| {
            let players: Vec<_> = c.conn.db.player().iter().collect();
            let online = players.iter().filter(|p| p.online).count();
            (online, players.len())
        })
        .unwrap_or((0, 0));

    if let Ok(mut text) = texts.single_mut() {
        let new = format!("{online} / {total}");
        if text.0 != new {
            text.0 = new;
        }
    }
}

fn tick_ping(
    tracker: Option<Res<PingTracker>>,
    mut texts: Query<&mut Text, With<PingText>>,
    mut colors: Query<&mut TextColor, With<PingText>>,
) {
    let Ok(mut text) = texts.single_mut() else {
        return;
    };

    let Some(tracker) = tracker.as_ref() else {
        if text.0 != "-- ms" {
            text.0 = "-- ms".to_string();
        }
        return;
    };

    let ms = tracker.smoothed_rtt_ms;
    let new = if ms > 0.0 {
        format!("{ms:.0} ms")
    } else {
        "-- ms".to_string()
    };
    if text.0 != new {
        text.0 = new;
    }

    // Color code: green < 80ms, yellow < 150ms, red >= 150ms
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

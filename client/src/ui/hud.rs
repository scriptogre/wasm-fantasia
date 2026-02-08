use bevy::prelude::*;

use crate::combat::Health;
use crate::models::{Player, Screen};
use crate::ui::colors::{HEALTH_RED, NEUTRAL300, NEUTRAL700, NEUTRAL920};
use crate::ui::size::{HEALTH_BAR_HEIGHT, HEALTH_BAR_WIDTH};

// ── Components ──────────────────────────────────────────────────────

#[derive(Component)]
struct PlayerHud;

#[derive(Component)]
struct HudHealthFill;

#[derive(Component)]
struct HudHealthText;

#[derive(Component)]
struct HudPlayerName;

// ── Font ────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct HudFont(pub Handle<Font>);

// ── Plugin ──────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, load_hud_font)
        .add_systems(OnEnter(Screen::Gameplay), spawn_hud)
        .add_systems(Update, (tick_health, tick_name));
}

fn load_hud_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(HudFont(
        asset_server.load("fonts/ShareTechMono-Regular.ttf"),
    ));
}

// ── Spawn ───────────────────────────────────────────────────────────

fn spawn_hud(mut commands: Commands, font: Res<HudFont>) {
    let font = font.0.clone();

    commands
        .spawn((
            PlayerHud,
            DespawnOnExit(Screen::Gameplay),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(32.0),
                bottom: Val::Px(32.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            GlobalZIndex(90),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            // Player name
            parent.spawn((
                HudPlayerName,
                Text::new("PLAYER"),
                TextFont {
                    font: font.clone(),
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    margin: UiRect::bottom(Val::Px(6.0)),
                    padding: UiRect::left(Val::Px(2.0)),
                    ..default()
                },
            ));

            // HP bar
            parent
                .spawn((
                    Node {
                        width: Val::Px(HEALTH_BAR_WIDTH),
                        height: Val::Px(HEALTH_BAR_HEIGHT),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(NEUTRAL920.with_alpha(0.8)),
                    BorderColor::all(NEUTRAL700.with_alpha(0.5)),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        HudHealthFill,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(HEALTH_RED),
                    ));
                });

            // HP label row
            parent
                .spawn(Node {
                    width: Val::Px(HEALTH_BAR_WIDTH),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    margin: UiRect::top(Val::Px(4.0)),
                    padding: UiRect::horizontal(Val::Px(2.0)),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("HP"),
                        TextFont {
                            font: font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(NEUTRAL300),
                    ));
                    row.spawn((
                        HudHealthText,
                        Text::new("100 / 100"),
                        TextFont {
                            font,
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

// ── Tick systems ────────────────────────────────────────────────────

fn tick_health(
    player: Query<&Health, With<Player>>,
    mut fills: Query<&mut Node, With<HudHealthFill>>,
    mut texts: Query<&mut Text, With<HudHealthText>>,
) {
    let Ok(health) = player.single() else { return };

    if let Ok(mut fill) = fills.single_mut() {
        fill.width = Val::Percent(health.fraction() * 100.0);
    }
    if let Ok(mut text) = texts.single_mut() {
        text.0 = format!("{:.0} / {:.0}", health.current, health.max);
    }
}

fn tick_name(
    player: Query<Option<&Name>, With<Player>>,
    mut names: Query<&mut Text, With<HudPlayerName>>,
) {
    let Ok(name_opt) = player.single() else { return };
    let display = name_opt.map(|n| n.as_str()).unwrap_or("PLAYER");

    if let Ok(mut text) = names.single_mut() {
        let upper = display.to_uppercase();
        if text.0 != upper {
            text.0 = upper;
        }
    }
}

use bevy::prelude::*;

use crate::combat::Health;
use crate::models::{Player, Screen};
use crate::ui::colors::{AMBER, NEUTRAL50, RED};
use crate::ui::size::{EDGE, HEALTH_BAR_HEIGHT, HEALTH_BAR_WIDTH};

// ── Components ──────────────────────────────────────────────────────

#[derive(Component)]
struct HudHealthFill;

#[derive(Component)]
struct HudHealthText;

/// Below this fraction the bar turns red
const LOW_HEALTH: f32 = 0.3;

// ── Plugin ──────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Gameplay), spawn_hud)
        .add_systems(Update, tick_health.run_if(in_state(Screen::Gameplay)));
}

// ── Spawn ───────────────────────────────────────────────────────────

fn spawn_hud(mut commands: Commands) {
    let pill = BorderRadius::all(Val::Px(HEALTH_BAR_HEIGHT / 2.0));
    commands.spawn((
        Name::new("Player HUD"),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(EDGE),
            top: Val::Px(EDGE),
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            ..default()
        },
        GlobalZIndex(90),
        Pickable::IGNORE,
        children![
            (
                Node {
                    width: Val::Px(HEALTH_BAR_WIDTH),
                    height: Val::Px(HEALTH_BAR_HEIGHT),
                    border_radius: pill,
                    ..default()
                },
                BackgroundColor(NEUTRAL50.with_alpha(0.14)),
                children![(
                    HudHealthFill,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        border_radius: pill,
                        ..default()
                    },
                    BackgroundColor(AMBER),
                )],
            ),
            (
                HudHealthText,
                Text::new("100"),
                TextFont::from_font_size(16.0),
                TextColor(NEUTRAL50),
                TextShadow {
                    offset: Vec2::new(0.0, 1.0),
                    color: Color::BLACK.with_alpha(0.6),
                },
            ),
        ],
    ));
}

// ── Tick systems ────────────────────────────────────────────────────

fn tick_health(
    player: Query<Ref<Health>, With<Player>>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), With<HudHealthFill>>,
    mut text: Single<&mut Text, With<HudHealthText>>,
) {
    let Ok(health) = player.single() else { return };
    if !health.is_changed() && !text.is_added() {
        return;
    }

    let fraction = health.fraction();
    fill.0.width = Val::Percent(fraction * 100.0);
    fill.1.0 = if fraction < LOW_HEALTH { RED } else { AMBER };
    text.0 = format!("{:.0}", health.current);
}

use bevy::prelude::*;

use crate::combat::Health;
use crate::models::{Player, Screen};
use crate::ui::colors::{ALERT, INK};
use crate::ui::fonts::SEMIBOLD;
use crate::ui::size::{EDGE, HEALTH_BAR_HEIGHT, HEALTH_BAR_WIDTH};

// ── Components ──────────────────────────────────────────────────────

#[derive(Component)]
struct HudHealthFill;

#[derive(Component)]
struct HudHealthText;

/// Below this fraction the bar turns red
const LOW_HEALTH: f32 = 0.3;

/// Keeps light HUD text legible over bright parts of the scene
pub const HUD_SHADOW: TextShadow = TextShadow {
    offset: Vec2::new(0.0, 1.0),
    color: Color::srgba(0.0, 0.0, 0.0, 0.7),
};

// ── Plugin ──────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Gameplay), spawn_hud)
        .add_systems(Update, tick_health.run_if(in_state(Screen::Gameplay)));
}

// ── Spawn ───────────────────────────────────────────────────────────

fn spawn_hud(mut commands: Commands) {
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
                    ..default()
                },
                BackgroundColor(INK.with_alpha(0.2)),
                children![(
                    HudHealthFill,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(INK),
                )],
            ),
            (
                HudHealthText,
                Text::new("100"),
                TextFont {
                    font: SEMIBOLD,
                    weight: bevy::text::FontWeight::SEMIBOLD,
                    font_size: 16.0,
                    ..default()
                },
                TextColor(INK),
                HUD_SHADOW,
            ),
        ],
    ));
}

// ── Tick systems ────────────────────────────────────────────────────

fn tick_health(
    player: Query<Ref<Health>, With<Player>>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), With<HudHealthFill>>,
    mut text: Single<(&mut Text, &mut TextColor), With<HudHealthText>>,
) {
    let Ok(health) = player.single() else { return };
    if !health.is_changed() && !text.0.is_added() {
        return;
    }

    let fraction = health.fraction();
    let color = if fraction < LOW_HEALTH { ALERT } else { INK };
    fill.0.width = Val::Percent(fraction * 100.0);
    fill.1.0 = color;
    text.0.0 = format!("{:.0}", health.current);
    text.1.0 = color;
}

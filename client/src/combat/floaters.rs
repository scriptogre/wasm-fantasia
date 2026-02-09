use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::combat::components::{Enemy, Health};
use crate::combat::{DamageDealt, Died, HitLanded};
use crate::models::SceneCamera;
use crate::ui::colors::{GRASS_GREEN, NEUTRAL450, NEUTRAL850, RED, SAND_YELLOW};

pub fn plugin(app: &mut App) {
    app.add_observer(on_damage_number)
        .add_observer(on_enemy_damaged)
        .add_observer(on_enemy_death)
        .add_systems(Startup, setup_glyph_cache)
        .add_systems(
            PostUpdate,
            (tick_damage_numbers, tick_enemy_health_bars).after(TransformSystems::Propagate),
        );
}

// ── Damage Numbers ──────────────────────────────────────────────────

#[derive(Component)]
pub struct GlyphCache;

#[derive(Component)]
pub struct DamageNumber {
    pub timer: f32,
    pub is_crit: bool,
    pub world_pos: Vec3,
    pub offset: Vec2,
}

pub const DAMAGE_COLOR: Color = crate::ui::colors::NEUTRAL10;
pub const CRIT_COLOR: Color = Color::oklcha(0.905, 0.182, 98.111, 1.0);

const DISPLAY_DURATION: f32 = 0.8;
const POP_DURATION: f32 = 0.15;
const HOLD_END: f32 = 0.4;
const RISE_PIXELS: f32 = 80.0;

fn setup_glyph_cache(mut commands: Commands) {
    for size in [20.0, 28.0] {
        commands.spawn((
            GlyphCache,
            Text::new("0123456789"),
            TextFont {
                font_size: size,
                ..default()
            },
            TextColor(Color::NONE),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(-9999.0),
                top: Val::Px(-9999.0),
                ..default()
            },
        ));
    }
}

fn on_damage_number(
    on: On<HitLanded>,
    targets: Query<&Transform>,
    fonts: Option<Res<crate::asset_loading::Fonts>>,
    mut commands: Commands,
) {
    let event = on.event();

    let Ok(target_transform) = targets.get(event.target) else {
        return;
    };

    let world_pos = target_transform.translation + Vec3::Y * 1.0;
    let damage = event.damage as i32;
    let is_crit = event.is_crit;

    let mut rng = rand::rng();
    let offset = Vec2::new(
        rand::Rng::random_range(&mut rng, -40.0..40.0),
        rand::Rng::random_range(&mut rng, -20.0..20.0),
    );

    let (font_size, font) = if is_crit {
        (28.0, fonts.as_ref().map(|f| f.bold.clone()))
    } else {
        (20.0, fonts.as_ref().map(|f| f.regular.clone()))
    };
    let mut text_font = TextFont::from_font_size(font_size);
    if let Some(handle) = font {
        text_font.font = handle;
    }

    commands.spawn((
        DamageNumber {
            timer: 0.0,
            is_crit,
            world_pos,
            offset,
        },
        Text::new(format!("{}", damage)),
        text_font,
        TextColor(if is_crit { CRIT_COLOR } else { DAMAGE_COLOR }),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(-9999.0),
            top: Val::Px(-9999.0),
            ..default()
        },
        GlobalZIndex(100),
        Pickable::IGNORE,
    ));
}

fn tick_damage_numbers(
    time: Res<Time>,
    mut commands: Commands,
    camera: Query<(&Camera, &GlobalTransform), With<SceneCamera>>,
    mut numbers: Query<(Entity, &mut DamageNumber, &mut Node, &mut TextColor)>,
) {
    let delta = time.delta_secs();

    let Ok((cam, cam_global)) = camera.single() else {
        return;
    };

    for (entity, mut dmg, mut node, mut color) in numbers.iter_mut() {
        dmg.timer += delta;
        let t = (dmg.timer / DISPLAY_DURATION).min(1.0);

        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let Some(base_screen) = cam.world_to_viewport(cam_global, dmg.world_pos).ok() else {
            node.left = Val::Px(-9999.0);
            node.top = Val::Px(-9999.0);
            continue;
        };

        let y_offset = if t < POP_DURATION / DISPLAY_DURATION {
            let pop_t = t / (POP_DURATION / DISPLAY_DURATION);
            let ease = 1.0 - (1.0 - pop_t).powi(3);
            let overshoot = if pop_t > 0.6 {
                1.0 + (1.0 - pop_t) * 0.4 * ((pop_t - 0.6) / 0.4).sin() * std::f32::consts::PI
            } else {
                ease
            };
            -40.0 * overshoot
        } else {
            let rise_t =
                (t - POP_DURATION / DISPLAY_DURATION) / (1.0 - POP_DURATION / DISPLAY_DURATION);
            -40.0 - (RISE_PIXELS - 40.0) * rise_t.sqrt()
        };

        node.left = Val::Px(base_screen.x + dmg.offset.x - 24.0);
        node.top = Val::Px(base_screen.y + dmg.offset.y + y_offset);

        let alpha = if t < HOLD_END {
            1.0
        } else {
            let fade_t = (t - HOLD_END) / (1.0 - HOLD_END);
            1.0 - fade_t * fade_t
        };

        let base_color = if dmg.is_crit {
            CRIT_COLOR
        } else {
            DAMAGE_COLOR
        };
        color.0 = base_color.with_alpha(alpha);
    }
}

// ── Enemy Health Bars ───────────────────────────────────────────────

const ENEMY_BAR_WIDTH: f32 = 60.0;
const ENEMY_BAR_HEIGHT: f32 = 6.0;
const VISIBILITY_DURATION: f32 = 3.0;

fn health_color(fraction: f32) -> Color {
    if fraction > 0.6 {
        GRASS_GREEN
    } else if fraction > 0.3 {
        SAND_YELLOW
    } else {
        RED
    }
}

#[derive(Component)]
pub struct EnemyHealthBar {
    pub target: Entity,
    pub visible_timer: f32,
}

#[derive(Component)]
pub struct HealthBarFill;

fn on_enemy_damaged(
    on: On<DamageDealt>,
    enemies: Query<&GlobalTransform, With<Enemy>>,
    mut health_bars: Query<&mut EnemyHealthBar>,
    mut commands: Commands,
) {
    let event = on.event();

    for mut bar in health_bars.iter_mut() {
        if bar.target == event.target {
            bar.visible_timer = VISIBILITY_DURATION;
            return;
        }
    }

    let Ok(_enemy_tf) = enemies.get(event.target) else {
        return;
    };

    commands
        .spawn((
            EnemyHealthBar {
                target: event.target,
                visible_timer: VISIBILITY_DURATION,
            },
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(ENEMY_BAR_WIDTH),
                height: Val::Px(ENEMY_BAR_HEIGHT),
                left: Val::Px(-9999.0),
                top: Val::Px(-9999.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(3.0)),
            BorderColor::all(NEUTRAL450.with_alpha(0.6)),
            BackgroundColor(NEUTRAL850.with_alpha(0.7)),
            GlobalZIndex(90),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            parent.spawn((
                HealthBarFill,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BorderRadius::all(Val::Px(2.0)),
                BackgroundColor(GRASS_GREEN),
            ));
        });
}

fn on_enemy_death(
    on: On<Died>,
    health_bars: Query<(Entity, &EnemyHealthBar)>,
    mut commands: Commands,
) {
    let event = on.event();

    for (entity, bar) in health_bars.iter() {
        if bar.target == event.entity {
            commands.entity(entity).despawn();
            return;
        }
    }
}

fn tick_enemy_health_bars(
    time: Res<Time>,
    mut commands: Commands,
    camera: Query<(&Camera, &GlobalTransform), With<SceneCamera>>,
    enemies: Query<(&GlobalTransform, &Health), With<Enemy>>,
    mut health_bars: Query<(
        Entity,
        &mut EnemyHealthBar,
        &mut Node,
        &mut BackgroundColor,
        &Children,
    )>,
    mut fills: Query<
        (&mut Node, &mut BackgroundColor),
        (With<HealthBarFill>, Without<EnemyHealthBar>),
    >,
) {
    let delta = time.delta_secs();

    let Ok((cam, cam_global)) = camera.single() else {
        return;
    };

    for (entity, mut bar, mut node, mut bg, children) in health_bars.iter_mut() {
        bar.visible_timer -= delta;

        if bar.visible_timer <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let Ok((enemy_tf, health)) = enemies.get(bar.target) else {
            commands.entity(entity).despawn();
            continue;
        };

        let world_pos = enemy_tf.translation() + Vec3::Y * 2.2;
        let Some(screen_pos) = cam.world_to_viewport(cam_global, world_pos).ok() else {
            node.left = Val::Px(-9999.0);
            node.top = Val::Px(-9999.0);
            continue;
        };

        node.left = Val::Px(screen_pos.x - ENEMY_BAR_WIDTH / 2.0);
        node.top = Val::Px(screen_pos.y);

        let alpha = if bar.visible_timer < 0.5 {
            bar.visible_timer / 0.5
        } else {
            1.0
        };
        bg.0 = NEUTRAL850.with_alpha(0.7 * alpha);

        let fraction = health.fraction();
        for child in children.iter() {
            if let Ok((mut fill_node, mut fill_bg)) = fills.get_mut(child) {
                fill_node.width = Val::Percent(fraction * 100.0);
                fill_bg.0 = health_color(fraction).with_alpha(alpha);
            }
        }
    }
}

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::combat::{DamageDealt, Died};
use crate::combat::{Enemy, Health};
use crate::models::SceneCamera;
use crate::networking::CombatEvent;
use crate::ui::colors::{AMBER, NEUTRAL100, VOID};

/// Cached mesh height above the entity origin, computed once from descendant AABBs.
#[derive(Component)]
pub struct MeshHeight(pub f32);

pub fn plugin(app: &mut App) {
    app.add_observer(on_server_damage_number)
        .add_observer(on_enemy_damaged)
        .add_observer(on_enemy_death)
        .add_systems(Startup, setup_glyph_cache)
        .add_systems(
            PostUpdate,
            (
                compute_mesh_heights,
                tick_damage_numbers,
                tick_enemy_health_bars,
            )
                .after(TransformSystems::Propagate),
        );
}

/// Derives [`MeshHeight`] from the highest descendant [`Aabb`] (auto-added by
/// Bevy to every [`Mesh3d`]). Runs once per enemy then caches.
fn compute_mesh_heights(
    mut commands: Commands,
    enemies: Query<(Entity, &GlobalTransform), (With<Enemy>, Without<MeshHeight>)>,
    children: Query<&Children>,
    aabbs: Query<(&GlobalTransform, &Aabb)>,
) {
    for (entity, enemy_gt) in &enemies {
        let origin_y = enemy_gt.translation().y;
        let top = children
            .iter_descendants(entity)
            .filter_map(|e| aabbs.get(e).ok())
            .map(|(gt, aabb)| {
                gt.transform_point(Vec3::Y * (aabb.center.y + aabb.half_extents.y))
                    .y
                    - origin_y
            })
            .reduce(f32::max);
        if let Some(height) = top {
            commands.entity(entity).insert(MeshHeight(height));
        }
    }
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
pub const CRIT_COLOR: Color = AMBER;

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

/// Spawns floating damage numbers from **server-confirmed** [`CombatEvent`].
///
/// Client-predicted effects (sound, flash, hit stop) remain on
/// [`HitLanded`](crate::combat::HitLanded) for responsiveness.
fn on_server_damage_number(on: On<CombatEvent>, mut commands: Commands) {
    let event = on.event();
    let world_pos = Vec3::new(event.x, event.y, event.z);
    let damage = event.damage as i32;
    let is_crit = event.is_crit;

    let mut rng = rand::rng();
    let offset = Vec2::new(
        rand::Rng::random_range(&mut rng, -40.0..40.0),
        rand::Rng::random_range(&mut rng, -20.0..20.0),
    );

    // Regular hits use the default font so the glyph cache prewarm applies
    let mut text_font = TextFont::from_font_size(if is_crit { 28.0 } else { 20.0 });
    if is_crit {
        text_font.font = crate::ui::fonts::SEMIBOLD;
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
        TextShadow {
            offset: Vec2::new(0.0, 2.0),
            color: Color::BLACK.with_alpha(0.6),
        },
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
    mut numbers: Query<(
        Entity,
        &mut DamageNumber,
        &mut Node,
        &mut TextColor,
        &mut TextShadow,
    )>,
) {
    let delta = time.delta_secs();

    let Ok((cam, cam_global)) = camera.single() else {
        return;
    };

    for (entity, mut dmg, mut node, mut color, mut shadow) in numbers.iter_mut() {
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
        shadow.color = Color::BLACK.with_alpha(0.6 * alpha);
    }
}

// ── Enemy Health Bars ───────────────────────────────────────────────

const ENEMY_BAR_WIDTH: f32 = 44.0;
const ENEMY_BAR_HEIGHT: f32 = 4.0;
const VISIBILITY_DURATION: f32 = 3.0;
const TRACK_ALPHA: f32 = 0.6;

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
                border_radius: BorderRadius::all(Val::Px(ENEMY_BAR_HEIGHT / 2.0)),
                ..default()
            },
            BackgroundColor(VOID.with_alpha(TRACK_ALPHA)),
            GlobalZIndex(90),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            parent.spawn((
                HealthBarFill,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    border_radius: BorderRadius::all(Val::Px(ENEMY_BAR_HEIGHT / 2.0)),
                    ..default()
                },
                BackgroundColor(NEUTRAL100),
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
    enemies: Query<(&GlobalTransform, &Health, Option<&MeshHeight>), With<Enemy>>,
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

        let Ok((enemy_tf, health, mesh_height)) = enemies.get(bar.target) else {
            commands.entity(entity).despawn();
            continue;
        };

        let world_pos = enemy_tf.translation() + Vec3::Y * (mesh_height.map_or(2.0, |h| h.0) + 0.2);
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
        bg.0 = VOID.with_alpha(TRACK_ALPHA * alpha);

        let fraction = health.fraction();
        for child in children.iter() {
            if let Ok((mut fill_node, mut fill_bg)) = fills.get_mut(child) {
                fill_node.width = Val::Percent(fraction * 100.0);
                fill_bg.0 = NEUTRAL100.with_alpha(alpha);
            }
        }
    }
}

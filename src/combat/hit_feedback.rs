use bevy::prelude::*;

use crate::models::SceneCamera;

pub fn plugin(app: &mut App) {
    app.insert_resource(HitStop::default())
        .insert_resource(ScreenShake::default())
        .add_observer(on_hit_stop)
        .add_observer(on_screen_shake)
        .add_observer(on_flash)
        .add_systems(
            Update,
            (tick_hit_stop, tick_screen_shake, tick_flash),
        );
}

/// Event fired when a hit connects, triggering feedback effects.
#[derive(Event, Debug, Clone)]
pub struct HitEvent {
    pub target: Entity,
    pub damage: f32,
}

// ============================================================================
// HIT STOP (Freeze Frame)
// ============================================================================

#[derive(Resource, Default)]
pub struct HitStop {
    pub remaining: f32,
    pub active: bool,
}

impl HitStop {
    pub const DURATION: f32 = 0.08; // ~5 frames at 60fps - more punch
}

fn on_hit_stop(
    _on: On<HitEvent>,
    mut hit_stop: ResMut<HitStop>,
    mut time: ResMut<Time<Virtual>>,
) {
    hit_stop.remaining = HitStop::DURATION;
    hit_stop.active = true;
    time.set_relative_speed(0.05); // Near-freeze
}

fn tick_hit_stop(
    real_time: Res<Time<Real>>,
    mut hit_stop: ResMut<HitStop>,
    mut time: ResMut<Time<Virtual>>,
) {
    if !hit_stop.active {
        return;
    }

    hit_stop.remaining -= real_time.delta_secs();

    if hit_stop.remaining <= 0.0 {
        hit_stop.active = false;
        hit_stop.remaining = 0.0;
        time.set_relative_speed(1.0);
    }
}

// ============================================================================
// SCREEN SHAKE
// ============================================================================

#[derive(Resource, Default)]
pub struct ScreenShake {
    pub trauma: f32,
}

impl ScreenShake {
    pub const DECAY: f32 = 4.0;
    pub const MAX_OFFSET: f32 = 0.5;
    pub const HIT_TRAUMA: f32 = 0.5;
}

fn on_screen_shake(_on: On<HitEvent>, mut shake: ResMut<ScreenShake>) {
    shake.trauma = (shake.trauma + ScreenShake::HIT_TRAUMA).min(1.0);
}

fn tick_screen_shake(
    time: Res<Time>,
    mut shake: ResMut<ScreenShake>,
    mut camera: Query<&mut Transform, With<SceneCamera>>,
) {
    if shake.trauma <= 0.0 {
        return;
    }

    shake.trauma = (shake.trauma - ScreenShake::DECAY * time.delta_secs()).max(0.0);
    let shake_amount = shake.trauma * shake.trauma;

    if let Ok(mut transform) = camera.single_mut() {
        let t = time.elapsed_secs() * 30.0;
        let offset_x = (t.sin() * 1.3 + (t * 2.7).cos()) * shake_amount * ScreenShake::MAX_OFFSET;
        let offset_y =
            ((t * 1.1).cos() * 1.5 + (t * 3.1).sin()) * shake_amount * ScreenShake::MAX_OFFSET;

        transform.translation.x += offset_x * time.delta_secs() * 10.0;
        transform.translation.y += offset_y * time.delta_secs() * 10.0;
    }
}

// ============================================================================
// WHITE FLASH
// ============================================================================

#[derive(Component)]
pub struct HitFlash {
    pub timer: Timer,
    pub original_color: Color,
}

fn on_flash(
    on: On<HitEvent>,
    mut commands: Commands,
    targets: Query<&MeshMaterial3d<StandardMaterial>, Without<HitFlash>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let event = on.event();

    if let Ok(mat_handle) = targets.get(event.target) {
        if let Some(mat) = materials.get(&mat_handle.0) {
            let original = mat.base_color;
            commands.entity(event.target).insert(HitFlash {
                timer: Timer::from_seconds(0.05, TimerMode::Once),
                original_color: original,
            });
            // Set to white
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.base_color = Color::WHITE;
            }
        }
    }
}

fn tick_flash(
    time: Res<Time>,
    mut commands: Commands,
    mut flashing: Query<(Entity, &mut HitFlash, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut flash, mat_handle) in flashing.iter_mut() {
        flash.timer.tick(time.delta());

        if flash.timer.is_finished() {
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.base_color = flash.original_color;
            }
            commands.entity(entity).remove::<HitFlash>();
        }
    }
}

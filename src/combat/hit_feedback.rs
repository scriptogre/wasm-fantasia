use bevy::prelude::*;

use crate::models::SceneCamera;

pub fn plugin(app: &mut App) {
    app.insert_resource(HitStop::default())
        .insert_resource(ScreenShake::default())
        .add_observer(on_hit_stop)
        .add_observer(on_screen_shake)
        .add_observer(on_impact_vfx)
        .add_systems(
            Update,
            (tick_hit_stop, tick_screen_shake, tick_impact_vfx),
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
    pub const DURATION: f32 = 0.05; // ~3 frames - quick punch
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
    pub const DECAY: f32 = 3.0;      // Slower decay = longer shake
    pub const MAX_OFFSET: f32 = 0.8; // Stronger shake
    pub const HIT_TRAUMA: f32 = 0.6; // More initial trauma
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
// IMPACT VFX (Modern/Sleek Style)
// ============================================================================

#[derive(Component)]
pub struct ImpactVfx {
    pub timer: Timer,
    pub start_scale: Vec3,
    pub end_scale: Vec3,
}

fn on_impact_vfx(
    on: On<HitEvent>,
    targets: Query<&Transform>,
    hand_bones: Query<(&Name, &GlobalTransform)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let event = on.event();

    let Ok(target_transform) = targets.get(event.target) else {
        return;
    };

    let target_pos = target_transform.translation;

    // Find the hand bone closest to the target - that's the one punching
    let impact_pos = hand_bones
        .iter()
        .filter(|(name, _)| {
            let n = name.as_str().to_lowercase();
            n.contains("hand")
        })
        .min_by(|(_, a), (_, b)| {
            let dist_a = a.translation().distance_squared(target_pos);
            let dist_b = b.translation().distance_squared(target_pos);
            dist_a.partial_cmp(&dist_b).unwrap()
        })
        .map(|(_, gt)| gt.translation())
        .unwrap_or_else(|| target_pos + Vec3::Y * 0.8);

    // Impact lines only (no sphere)
    let line_mesh = meshes.add(Cuboid::new(0.08, 0.08, 0.6));
    let line_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.8, 0.3, 1.0),
        emissive: LinearRgba::new(8.0, 5.0, 1.0, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    // Spawn 8 lines at different angles
    for i in 0..8 {
        let angle = (i as f32 / 8.0) * std::f32::consts::TAU;
        let dir = Vec3::new(angle.cos(), 0.0, angle.sin()).normalize();
        let rotation = Quat::from_rotation_arc(Vec3::Z, dir);

        commands.spawn((
            Mesh3d(line_mesh.clone()),
            MeshMaterial3d(line_mat.clone()),
            Transform::from_translation(impact_pos + dir * 0.3)
                .with_rotation(rotation)
                .with_scale(Vec3::new(1.0, 1.0, 0.3)),
            ImpactVfx {
                timer: Timer::from_seconds(0.12, TimerMode::Once),
                start_scale: Vec3::new(1.0, 1.0, 0.3),
                end_scale: Vec3::new(0.3, 0.3, 2.5),
            },
        ));
    }
}

fn tick_impact_vfx(
    time: Res<Time>,
    mut commands: Commands,
    mut vfx: Query<(Entity, &mut ImpactVfx, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut impact, mut transform, mat_handle) in vfx.iter_mut() {
        impact.timer.tick(time.delta());

        let progress = impact.timer.fraction();
        // Ease out for snappy feel
        let eased = 1.0 - (1.0 - progress).powi(2);

        let scale = impact.start_scale.lerp(impact.end_scale, eased);
        transform.scale = scale;

        // Quick fade out
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            let alpha = (1.0 - eased).max(0.0);
            mat.base_color = mat.base_color.with_alpha(alpha);
        }

        if impact.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

use bevy::prelude::*;

use crate::models::SceneCamera;

pub fn plugin(app: &mut App) {
    app.insert_resource(HitStop::default())
        .insert_resource(ScreenShake::default())
        .insert_resource(DamageNumberCooldown::default())
        .add_observer(on_hit_stop)
        .add_observer(on_screen_shake)
        .add_observer(on_impact_vfx)
        .add_observer(on_damage_number)
        .add_systems(
            Update,
            (tick_hit_stop, tick_screen_shake, tick_impact_vfx, tick_damage_numbers),
        );
}

/// Event fired when a hit connects, triggering feedback effects.
#[derive(Event, Debug, Clone)]
pub struct HitEvent {
    pub target: Entity,
    pub damage: f32,
    pub is_crit: bool,
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

// ============================================================================
// DAMAGE NUMBERS
// ============================================================================

/// Damage number colors
pub const DAMAGE_COLOR: Color = Color::srgb(1.0, 0.85, 0.2); // Gold for normal hits
pub const DAMAGE_SHADOW_COLOR: Color = Color::srgb(0.2, 0.1, 0.0);
pub const CRIT_COLOR: Color = Color::srgb(1.0, 0.3, 0.2); // Red-orange for crits
pub const CRIT_SHADOW_COLOR: Color = Color::srgb(0.3, 0.05, 0.0);

/// Cooldown to prevent duplicate damage numbers from animation blend events
#[derive(Resource, Default)]
pub struct DamageNumberCooldown {
    pub last_hit_time: f32,
    pub last_target: Option<Entity>,
}

impl DamageNumberCooldown {
    /// Minimum time between damage numbers on the SAME target
    pub const MIN_INTERVAL: f32 = 0.1;
}

#[derive(Component)]
pub struct DamageNumber {
    pub timer: Timer,
    pub world_pos: Vec3,
    pub start_pos: Vec3,
    pub is_crit: bool,
}

/// Marker for shadow text (rendered with offset)
#[derive(Component)]
pub struct DamageNumberShadow;

/// Animation phases for damage numbers
impl DamageNumber {
    pub const TOTAL_DURATION: f32 = 0.9;
    pub const POP_DURATION: f32 = 0.06;   // Very fast pop
    pub const OVERSHOOT_DURATION: f32 = 0.08; // Bounce back from overshoot
    pub const HOLD_DURATION: f32 = 0.2;   // Hold at peak
    pub const FADE_DURATION: f32 = 0.56;  // Fade out
    pub const RISE_AMOUNT: f32 = 1.5;     // How far it floats up
    pub const MIN_SCALE: f32 = 0.0;       // Start invisible
    pub const OVERSHOOT_SCALE: f32 = 1.4; // Pop bigger than final
    pub const MAX_SCALE: f32 = 1.0;       // Settle to this
}

fn on_damage_number(
    on: On<HitEvent>,
    targets: Query<&Transform>,
    mut cooldown: ResMut<DamageNumberCooldown>,
    time: Res<Time<Real>>,
    mut commands: Commands,
) {
    let event = on.event();
    let now = time.elapsed_secs();
    let delta = now - cooldown.last_hit_time;

    // Prevent duplicate damage numbers within cooldown window
    if delta < DamageNumberCooldown::MIN_INTERVAL {
        return;
    }

    cooldown.last_hit_time = now;
    cooldown.last_target = Some(event.target);

    let Ok(target_transform) = targets.get(event.target) else {
        return;
    };

    // Spawn at target's chest height
    let world_pos = target_transform.translation + Vec3::Y * 1.4;
    let is_crit = event.is_crit;

    let damage_text = format!("{}", event.damage as i32);
    let base_font_size = if is_crit { 80.0 } else { 64.0 };
    let color = if is_crit { CRIT_COLOR } else { DAMAGE_COLOR };

    commands.spawn((
        DamageNumber {
            timer: Timer::from_seconds(DamageNumber::TOTAL_DURATION, TimerMode::Once),
            world_pos,
            start_pos: world_pos,
            is_crit,
        },
        Text::new(damage_text),
        TextFont {
            font_size: base_font_size,
            ..default()
        },
        TextColor(color),
        TextLayout::new_with_justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
    ));
}

fn tick_damage_numbers(
    time: Res<Time>,
    mut commands: Commands,
    camera: Query<(&Camera, &GlobalTransform), With<SceneCamera>>,
    mut numbers: Query<(Entity, &mut DamageNumber, &mut Node, &mut TextColor, &mut TextFont)>,
) {
    let Ok((camera, camera_transform)) = camera.single() else {
        return;
    };

    for (entity, mut dmg, mut node, mut color, mut font) in numbers.iter_mut() {
        dmg.timer.tick(time.delta());
        let elapsed = dmg.timer.elapsed_secs();

        let phase1_end = DamageNumber::POP_DURATION;
        let phase2_end = phase1_end + DamageNumber::OVERSHOOT_DURATION;
        let phase3_end = phase2_end + DamageNumber::HOLD_DURATION;

        // Phase-based animation with overshoot bounce
        let (scale, alpha, rise_progress) = if elapsed < phase1_end {
            // Phase 1: Explosive pop to overshoot
            let t = elapsed / DamageNumber::POP_DURATION;
            let eased = 1.0 - (1.0 - t).powi(2); // Ease out quad
            let scale = DamageNumber::MIN_SCALE + (DamageNumber::OVERSHOOT_SCALE - DamageNumber::MIN_SCALE) * eased;
            (scale, 1.0, 0.0)
        } else if elapsed < phase2_end {
            // Phase 2: Settle back from overshoot (bounce)
            let t = (elapsed - phase1_end) / DamageNumber::OVERSHOOT_DURATION;
            let eased = t * t; // Ease in
            let scale = DamageNumber::OVERSHOOT_SCALE - (DamageNumber::OVERSHOOT_SCALE - DamageNumber::MAX_SCALE) * eased;
            (scale, 1.0, t * 0.05)
        } else if elapsed < phase3_end {
            // Phase 3: Hold at peak
            let _t = (elapsed - phase2_end) / DamageNumber::HOLD_DURATION;
            (DamageNumber::MAX_SCALE, 1.0, 0.15)
        } else {
            // Phase 4: Rise and fade out
            let t = (elapsed - phase3_end) / DamageNumber::FADE_DURATION;
            let t_clamped = t.min(1.0);
            let rise_eased = 1.0 - (1.0 - t_clamped).powi(2);
            let alpha_eased = (1.0 - t_clamped).powi(3);
            let scale = DamageNumber::MAX_SCALE * (1.0 - t_clamped * 0.2);
            (scale, alpha_eased, 0.15 + rise_eased * 0.85)
        };

        // Update world position (smooth rise)
        dmg.world_pos = dmg.start_pos + Vec3::Y * (rise_progress * DamageNumber::RISE_AMOUNT);

        // Project to screen space
        if let Ok(screen_pos) = camera.world_to_viewport(camera_transform, dmg.world_pos) {
            node.left = Val::Px(screen_pos.x - 30.0);
            node.top = Val::Px(screen_pos.y - 24.0);
        }

        // Apply scale via font size (crits are bigger)
        let base_font_size = if dmg.is_crit { 80.0 } else { 64.0 };
        font.font_size = base_font_size * scale;

        // Apply alpha
        let base_color = if dmg.is_crit { CRIT_COLOR } else { DAMAGE_COLOR };
        color.0 = base_color.with_alpha(alpha);

        if dmg.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

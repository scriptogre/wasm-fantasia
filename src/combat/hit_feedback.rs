use bevy::prelude::*;

use crate::models::SceneCamera;

pub fn plugin(app: &mut App) {
    app.insert_resource(HitStop::default())
        .insert_resource(ScreenShake::default())
        .insert_resource(DamageDisplayState::default())
        .add_observer(on_hit_stop)
        .add_observer(on_screen_shake)
        .add_observer(on_damage_number)
        .add_systems(Startup, setup_damage_number_pool)
        .add_systems(Update, (tick_hit_stop, tick_screen_shake, tick_damage_numbers));

    // Impact VFX only on native (spawns entities per hit)
    #[cfg(not(target_arch = "wasm32"))]
    {
        app.add_observer(on_impact_vfx)
            .add_systems(Startup, setup_impact_assets)
            .add_systems(Update, tick_impact_vfx);
    }
}

/// Pre-created assets for impact VFX to avoid memory leaks on WASM
#[derive(Resource)]
pub struct ImpactAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

fn setup_impact_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(0.08, 0.08, 0.6));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.8, 0.3, 1.0),
        emissive: LinearRgba::new(8.0, 5.0, 1.0, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.insert_resource(ImpactAssets { mesh, material });
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
    impact_assets: Option<Res<ImpactAssets>>,
    mut commands: Commands,
) {
    let Some(assets) = impact_assets else {
        return;
    };

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

    // Spawn 8 lines at different angles (reusing pre-created assets)
    for i in 0..8 {
        let angle = (i as f32 / 8.0) * std::f32::consts::TAU;
        let dir = Vec3::new(angle.cos(), 0.0, angle.sin()).normalize();
        let rotation = Quat::from_rotation_arc(Vec3::Z, dir);

        commands.spawn((
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.material.clone()),
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
    mut vfx: Query<(Entity, &mut ImpactVfx, &mut Transform)>,
) {
    for (entity, mut impact, mut transform) in vfx.iter_mut() {
        impact.timer.tick(time.delta());

        let progress = impact.timer.fraction();
        // Ease out for snappy feel
        let eased = 1.0 - (1.0 - progress).powi(2);

        // Scale-based animation (stretches and shrinks to zero for fade effect)
        let mut scale = impact.start_scale.lerp(impact.end_scale, eased);
        // Shrink to zero as it fades (simulates alpha fade via scale)
        let fade = (1.0 - eased).max(0.0);
        scale *= fade;
        transform.scale = scale;

        if impact.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

// ============================================================================
// DAMAGE NUMBERS
// ============================================================================

/// Single damage display - shows latest hit, no spawning
#[derive(Component)]
pub struct DamageDisplay;

/// State tracked in resource to avoid component queries
#[derive(Resource, Default)]
pub struct DamageDisplayState {
    pub active: bool,
    pub timer: f32,
    pub damage: i32,
    pub is_crit: bool,
    pub world_pos: Vec3,
}

pub const DAMAGE_COLOR: Color = Color::srgb(0.0, 1.0, 0.0); // GREEN - test color
pub const CRIT_COLOR: Color = Color::srgb(0.0, 0.5, 1.0); // BLUE - test color

const DISPLAY_DURATION: f32 = 0.6;

fn setup_damage_number_pool(mut commands: Commands) {
    // Single pre-created text entity
    commands.spawn((
        DamageDisplay,
        Text::new(""),
        TextFont {
            font_size: 64.0,
            ..default()
        },
        TextColor(Color::NONE),
        TextLayout::new_with_justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
    ));
}

fn on_damage_number(
    on: On<HitEvent>,
    targets: Query<&Transform>,
    mut state: ResMut<DamageDisplayState>,
    mut display: Query<&mut Text, With<DamageDisplay>>,
) {
    let event = on.event();

    let Ok(target_transform) = targets.get(event.target) else {
        return;
    };

    // Update state
    state.active = true;
    state.timer = 0.0;
    state.damage = event.damage as i32;
    state.is_crit = event.is_crit;
    state.world_pos = target_transform.translation + Vec3::Y * 1.8;

    // Update text
    if let Ok(mut text) = display.single_mut() {
        **text = format!("{}", state.damage);
    }
}

fn tick_damage_numbers(
    time: Res<Time>,
    mut state: ResMut<DamageDisplayState>,
    camera: Query<(&Camera, &GlobalTransform), With<SceneCamera>>,
    mut display: Query<(&mut Node, &mut TextColor, &mut TextFont), With<DamageDisplay>>,
) {
    let Ok((camera, cam_transform)) = camera.single() else {
        return;
    };
    let Ok((mut node, mut color, mut font)) = display.single_mut() else {
        return;
    };

    if !state.active {
        color.0 = Color::NONE;
        return;
    }

    state.timer += time.delta_secs();
    let t = (state.timer / DISPLAY_DURATION).min(1.0);

    if t >= 1.0 {
        state.active = false;
        color.0 = Color::NONE;
        return;
    }

    // Animation: pop in, hold, fade out
    let scale = if t < 0.1 {
        // Pop in
        (t / 0.1) * 1.3
    } else if t < 0.4 {
        // Settle
        1.3 - (t - 0.1) * 0.3
    } else {
        // Shrink out
        1.0 - (t - 0.4) * 0.5
    };

    let alpha = if t < 0.6 { 1.0 } else { 1.0 - (t - 0.6) / 0.4 };

    // Rise
    let rise = t * 1.5;
    let pos = state.world_pos + Vec3::Y * rise;

    // Project to screen
    if let Ok(screen_pos) = camera.world_to_viewport(cam_transform, pos) {
        node.left = Val::Px(screen_pos.x - 50.0);
        node.top = Val::Px(screen_pos.y - 35.0);
    }

    let base_size = if state.is_crit { 80.0 } else { 64.0 };
    font.font_size = base_size * scale;

    let base_color = if state.is_crit { CRIT_COLOR } else { DAMAGE_COLOR };
    color.0 = base_color.with_alpha(alpha);
}

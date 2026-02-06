use std::time::Duration;

use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::combat::components::{DamageEvent, DeathEvent, Enemy, Health};
use crate::combat::{AttackConnect, VFX_ARC_DEGREES, VFX_RANGE};
use crate::models::{GameState, Player, SceneCamera, Screen};
use crate::rules::{Stat, Stats};
use crate::ui::colors::{GRASS_GREEN, GRAY_0, LIGHT_GRAY_1, RED, SAND_YELLOW};

pub fn plugin(app: &mut App) {
    app.insert_resource(HitStop::default())
        .insert_resource(ScreenShake::default())
        .add_observer(on_hit_stop)
        .add_observer(on_screen_shake)
        .add_observer(on_damage_number)
        .add_observer(on_hit_flash)
        .add_observer(on_phantom_fist)
        .add_observer(on_debug_hitbox)
        .add_observer(on_enemy_damaged)
        .add_observer(on_enemy_death)
        .add_observer(on_rumble)
        .add_systems(
            Startup,
            (
                setup_damage_number_pool,
                setup_phantom_fist_assets,
                setup_debug_hitbox_assets,
            ),
        )
        .add_systems(
            Update,
            (
                tick_hit_stop,
                tick_hit_flash,
                tick_phantom_fist,
                tick_debug_hitbox,
                tick_player_health_bar,
                tick_combat_stacks_display,
            ),
        )
        // Screen shake: restore original in PreUpdate, apply shake in PostUpdate before rendering
        .add_systems(PreUpdate, reset_camera_shake)
        .add_systems(
            PostUpdate,
            (
                apply_camera_shake.before(TransformSystems::Propagate),
                // World-to-screen projection needs GlobalTransform to be propagated first
                (tick_damage_numbers, tick_enemy_health_bars).after(TransformSystems::Propagate),
            ),
        );

    // Impact VFX (mesh-based sparks) - native only
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

// Mesh-based impact sparks (native only)
#[cfg(not(target_arch = "wasm32"))]
fn setup_impact_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(0.15, 0.15, 0.5));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.9, 0.5, 1.0),
        emissive: LinearRgba::new(15.0, 10.0, 2.0, 1.0),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        ..default()
    });
    commands.insert_resource(ImpactAssets { mesh, material });
}

// ============================================================================
// ARC SLASH (Visual indicator of attack sweep)
// Uses StandardMaterial with animated scale and alpha
// ============================================================================

#[derive(Resource)]
pub struct ArcSlashAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct ArcSlash {
    pub timer: f32,
    pub duration: f32,
    pub start_scale: Vec3,
}

fn setup_phantom_fist_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create arc mesh using shared VFX constants (hitbox is slightly larger)
    let arc_mesh = create_arc_mesh(
        VFX_RANGE,                    // radius matches visual range
        VFX_ARC_DEGREES.to_radians(), // arc matches visual arc
        0.6,                          // height
        16,                           // segments
    );
    let mesh = meshes.add(arc_mesh);

    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.7, 0.85, 1.0, 0.4),
        emissive: LinearRgba::new(2.0, 3.0, 5.0, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    });

    commands.insert_resource(ArcSlashAssets { mesh, material });
}

/// Creates a curved arc mesh for the slash effect
/// Arc faces -Z direction (forward in Bevy)
fn create_arc_mesh(radius: f32, arc_angle: f32, height: f32, segments: u32) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let half_angle = arc_angle / 2.0;
    let half_height = height / 2.0;

    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let angle = -half_angle + t * arc_angle;

        let x = angle.sin() * radius;
        let z = -angle.cos() * radius; // Negative Z = forward in Bevy

        // Bottom vertex
        positions.push([x, -half_height, z]);
        normals.push([0.0, 0.0, -1.0]); // Facing forward
        uvs.push([t, 0.0]);

        // Top vertex
        positions.push([x, half_height, z]);
        normals.push([0.0, 0.0, -1.0]);
        uvs.push([t, 1.0]);
    }

    for i in 0..segments {
        let base = i * 2;
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
        indices.push(base + 2);
        indices.push(base + 1);
        indices.push(base + 3);
    }

    Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(bevy::mesh::Indices::U32(indices))
}

fn on_phantom_fist(
    _on: On<AttackConnect>,
    player: Query<&Transform, With<crate::models::Player>>,
    assets: Option<Res<ArcSlashAssets>>,
    mut commands: Commands,
) {
    let Some(assets) = assets else {
        return;
    };

    let Ok(player_tf) = player.single() else {
        return;
    };

    // Spawn arc slash centered on player, matching their rotation
    let pos = player_tf.translation + Vec3::Y * 0.8;

    commands.spawn((
        ArcSlash {
            timer: 0.0,
            duration: 0.15,
            start_scale: Vec3::new(0.3, 1.0, 0.3),
        },
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material.clone()),
        Transform::from_translation(pos)
            .with_rotation(player_tf.rotation)
            .with_scale(Vec3::new(0.3, 1.0, 0.3)),
    ));
}

fn tick_phantom_fist(
    time: Res<Time>,
    mut commands: Commands,
    mut slashes: Query<(Entity, &mut ArcSlash, &mut Transform)>,
) {
    for (entity, mut slash, mut transform) in slashes.iter_mut() {
        slash.timer += time.delta_secs();
        let t = (slash.timer / slash.duration).min(1.0);

        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // Expand outward quickly (ease out)
        let eased = 1.0 - (1.0 - t).powi(3);
        let scale = slash.start_scale.lerp(Vec3::ONE, eased);
        transform.scale = scale;
    }
}

// ============================================================================
// DEBUG HITBOX VISUALIZATION
// Shows the actual attack hitbox (larger than visual) when debug_ui is enabled
// ============================================================================

#[derive(Resource)]
pub struct DebugHitboxAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct DebugHitbox {
    pub timer: f32,
    pub duration: f32,
}

fn setup_debug_hitbox_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Debug hitbox uses default values (actual values read from Stats at runtime)
    const DEBUG_RANGE: f32 = 3.6;
    const DEBUG_ARC: f32 = 150.0;

    // Create arc mesh using actual hitbox values (larger than visual)
    let arc_mesh = create_arc_mesh(
        DEBUG_RANGE,            // actual hitbox range
        DEBUG_ARC.to_radians(), // actual hitbox arc
        0.1,                    // thin height (wireframe-like)
        24,                     // more segments for accuracy
    );
    let mesh = meshes.add(arc_mesh);

    // Red semi-transparent for debug
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.2, 0.2, 0.3),
        emissive: LinearRgba::new(1.0, 0.0, 0.0, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    });

    commands.insert_resource(DebugHitboxAssets { mesh, material });
}

fn on_debug_hitbox(
    _on: On<AttackConnect>,
    game_state: Res<GameState>,
    player: Query<&Transform, With<crate::models::Player>>,
    assets: Option<Res<DebugHitboxAssets>>,
    mut commands: Commands,
) {
    // Only show when debug_ui is enabled
    if !game_state.debug_ui {
        return;
    }

    let Some(assets) = assets else {
        return;
    };

    let Ok(player_tf) = player.single() else {
        return;
    };

    // Spawn at player's feet level to show ground coverage
    let pos = player_tf.translation + Vec3::Y * 0.1;

    commands.spawn((
        DebugHitbox {
            timer: 0.0,
            duration: 0.5, // Show longer than VFX for debugging
        },
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material.clone()),
        Transform::from_translation(pos).with_rotation(player_tf.rotation),
    ));
}

fn tick_debug_hitbox(
    time: Res<Time>,
    mut commands: Commands,
    mut hitboxes: Query<(Entity, &mut DebugHitbox)>,
) {
    for (entity, mut hitbox) in hitboxes.iter_mut() {
        hitbox.timer += time.delta_secs();

        if hitbox.timer >= hitbox.duration {
            commands.entity(entity).despawn();
        }
    }
}

/// Event fired when a hit connects, triggering feedback effects.
#[derive(Event, Debug, Clone)]
pub struct HitEvent {
    pub source: Entity,
    pub target: Entity,
    pub damage: f32,
    pub is_crit: bool,
    /// Feedback configuration computed by rules.
    pub feedback: super::HitFeedback,
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
    pub const MAX_DURATION: f32 = 0.12; // Cap total accumulated freeze
}

fn on_hit_stop(
    on: On<HitEvent>,
    mut hit_stop: ResMut<HitStop>,
    mut time: ResMut<Time<Virtual>>,
    player: Query<&Stats, With<Player>>,
) {
    let event = on.event();
    let duration = event.feedback.hit_stop_duration;

    if duration <= 0.0 {
        return;
    }

    // Reduce freeze at high attack speed (flow state)
    let speed_reduction = if event.is_crit {
        0.0 // Crits ignore speed reduction
    } else {
        let attack_speed = player
            .single()
            .map(|s| s.get(&Stat::AttackSpeed))
            .unwrap_or(1.0);
        ((attack_speed - 1.0) * 0.5).clamp(0.0, 0.8)
    };

    let adjusted = (duration * (1.0 - speed_reduction)).max(0.01);
    hit_stop.remaining = (hit_stop.remaining + adjusted).min(HitStop::MAX_DURATION);
    hit_stop.active = true;
    time.set_relative_speed(0.05);
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
// Following Bevy's recommended pattern:
// - PreUpdate: restore camera to original position
// - PostUpdate (before Propagate): apply shake offset for rendering only
// ============================================================================

#[derive(Resource, Default)]
pub struct ScreenShake {
    pub trauma: f32,
    /// Stored camera transform from before shake was applied (only Some if shake is active)
    pub stored_transform: Option<Transform>,
}

impl ScreenShake {
    pub const DECAY: f32 = 2.5;
    pub const MAX_TRANSLATION: f32 = 0.25;
    pub const NOISE_SPEED: f32 = 25.0;
    pub const EXPONENT: f32 = 2.0;
}

fn on_screen_shake(on: On<HitEvent>, mut shake: ResMut<ScreenShake>) {
    let intensity = on.event().feedback.shake_intensity;
    shake.trauma = (shake.trauma + intensity).min(1.0);
}

// ============================================================================
// GAMEPAD RUMBLE
// Light pulse on hit, stronger on crit. Uses both motors for fuller feedback.
// ============================================================================

fn on_rumble(
    on: On<HitEvent>,
    gamepads: Query<Entity, With<Gamepad>>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
) {
    let feedback = &on.event().feedback;

    // Skip if no rumble configured
    if feedback.rumble_strong <= 0.0 && feedback.rumble_weak <= 0.0 {
        return;
    }

    for gamepad in gamepads.iter() {
        rumble.write(GamepadRumbleRequest::Add {
            gamepad,
            duration: Duration::from_millis(feedback.rumble_duration as u64),
            intensity: GamepadRumbleIntensity {
                strong_motor: feedback.rumble_strong,
                weak_motor: feedback.rumble_weak,
            },
        });
    }
}

/// Restore camera to its original (unshaken) position at start of frame.
fn reset_camera_shake(
    mut shake: ResMut<ScreenShake>,
    mut camera: Query<&mut Transform, With<SceneCamera>>,
) {
    // Only restore if we stored a transform (meaning shake was applied last frame)
    if let Some(original) = shake.stored_transform.take() {
        if let Ok(mut transform) = camera.single_mut() {
            *transform = original;
        }
    }
}

/// Apply screen shake offset just before rendering.
fn apply_camera_shake(
    time: Res<Time>,
    game_state: Res<crate::models::GameState>,
    mut shake: ResMut<ScreenShake>,
    mut camera: Query<&mut Transform, With<SceneCamera>>,
) {
    // Decay trauma
    shake.trauma = (shake.trauma - ScreenShake::DECAY * time.delta_secs()).max(0.0);

    // Skip if shake disabled or no trauma
    if !game_state.screen_shake || shake.trauma <= 0.0 {
        return;
    }

    let Ok(mut transform) = camera.single_mut() else {
        return;
    };

    // Store original BEFORE applying shake (only when we're about to shake)
    shake.stored_transform = Some(*transform);

    let shake_amount = shake.trauma.powf(ScreenShake::EXPONENT);
    let t = time.elapsed_secs() * ScreenShake::NOISE_SPEED;

    let x_noise = (t * 1.0).sin() * 0.5 + (t * 2.3).cos() * 0.3 + (t * 4.1).sin() * 0.2;
    let y_noise = (t * 1.7).cos() * 0.5 + (t * 3.1).sin() * 0.3 + (t * 5.3).cos() * 0.2;

    transform.translation.x += x_noise * shake_amount * ScreenShake::MAX_TRANSLATION;
    transform.translation.y += y_noise * shake_amount * ScreenShake::MAX_TRANSLATION;
}

// ============================================================================
// HIT FLASH (Enemy color flash on damage)
// ============================================================================

#[derive(Component)]
pub struct HitFlash {
    pub timer: f32,
    pub duration: f32,
    pub original_color: Color,
}

impl HitFlash {
    pub const FLASH_COLOR: Color = Color::srgb(1.0, 0.9, 0.8);
}

fn on_hit_flash(
    on: On<HitEvent>,
    mut targets: Query<(&MeshMaterial3d<StandardMaterial>, Option<&HitFlash>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let event = on.event();

    // Skip if no flash configured
    if event.feedback.flash_duration <= 0.0 {
        return;
    }

    let Ok((mat_handle, existing_flash)) = targets.get_mut(event.target) else {
        return;
    };

    // Don't stack flashes
    if existing_flash.is_some() {
        return;
    }

    let Some(material) = materials.get_mut(mat_handle) else {
        return;
    };

    // Store original and apply flash
    let original_color = material.base_color;
    material.base_color = HitFlash::FLASH_COLOR;
    material.emissive = LinearRgba::new(2.0, 1.8, 1.5, 1.0);

    commands.entity(event.target).insert(HitFlash {
        timer: 0.0,
        duration: event.feedback.flash_duration,
        original_color,
    });
}

fn tick_hit_flash(
    time: Res<Time>,
    mut flashing: Query<(Entity, &mut HitFlash, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (entity, mut flash, mat_handle) in flashing.iter_mut() {
        flash.timer += time.delta_secs();

        if flash.timer >= flash.duration {
            // Restore original color
            if let Some(material) = materials.get_mut(mat_handle) {
                material.base_color = flash.original_color;
                material.emissive = LinearRgba::NONE;
            }
            commands.entity(entity).remove::<HitFlash>();
        }
    }
}

// ============================================================================
// IMPACT VFX - Explosive burst on hit (native only)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
pub struct ImpactVfx {
    pub timer: f32,
    pub duration: f32,
    pub direction: Vec3,
    pub speed: f32,
    pub start_pos: Vec3,
}

#[cfg(not(target_arch = "wasm32"))]
fn on_impact_vfx(
    on: On<HitEvent>,
    targets: Query<&Transform>,
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

    // Impact at enemy's chest/center
    let impact_pos = target_transform.translation + Vec3::Y * 0.9;

    // Spawn particles in an explosive radial burst
    let num_particles = 12;
    let mut rng = rand::rng();

    for i in 0..num_particles {
        // Distribute evenly around a circle with some vertical spread
        let angle = (i as f32 / num_particles as f32) * std::f32::consts::TAU;
        let vertical = rand::Rng::random_range(&mut rng, -0.3..0.5);

        let dir = Vec3::new(angle.cos(), vertical, angle.sin()).normalize();
        let speed = rand::Rng::random_range(&mut rng, 3.0..6.0);
        let duration = rand::Rng::random_range(&mut rng, 0.15..0.25);

        // Orient the particle to face outward
        let rotation = Quat::from_rotation_arc(Vec3::Z, dir);

        commands.spawn((
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.material.clone()),
            Transform::from_translation(impact_pos)
                .with_rotation(rotation)
                .with_scale(Vec3::new(0.4, 0.4, 0.8)),
            ImpactVfx {
                timer: 0.0,
                duration,
                direction: dir,
                speed,
                start_pos: impact_pos,
            },
        ));
    }

    // Central flash burst - larger, stationary
    commands.spawn((
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material.clone()),
        Transform::from_translation(impact_pos).with_scale(Vec3::splat(0.1)),
        ImpactBurst {
            timer: 0.0,
            duration: 0.12,
        },
    ));
}

/// Central flash that expands and fades
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
pub struct ImpactBurst {
    pub timer: f32,
    pub duration: f32,
}

#[cfg(not(target_arch = "wasm32"))]
fn tick_impact_vfx(
    time: Res<Time>,
    mut commands: Commands,
    mut particles: Query<(Entity, &mut ImpactVfx, &mut Transform), Without<ImpactBurst>>,
    mut bursts: Query<(Entity, &mut ImpactBurst, &mut Transform)>,
) {
    let dt = time.delta_secs();

    // Animate outward-flying particles
    for (entity, mut vfx, mut transform) in particles.iter_mut() {
        vfx.timer += dt;
        let t = (vfx.timer / vfx.duration).min(1.0);

        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // Move outward
        let distance = vfx.speed * vfx.timer;
        transform.translation = vfx.start_pos + vfx.direction * distance;

        // Stretch in direction of travel, shrink as it fades
        let fade = 1.0 - t;
        let stretch = 1.0 + t * 2.0; // Elongate over time
        transform.scale = Vec3::new(0.3 * fade, 0.3 * fade, 0.8 * stretch * fade);
    }

    // Animate central burst
    for (entity, mut burst, mut transform) in bursts.iter_mut() {
        burst.timer += dt;
        let t = (burst.timer / burst.duration).min(1.0);

        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // Expand quickly then fade
        let ease = 1.0 - (1.0 - t).powi(3); // Ease out
        let scale = 0.1 + ease * 1.5; // Expand from 0.1 to 1.6
        let fade = 1.0 - t;
        transform.scale = Vec3::splat(scale * fade);
    }
}

// ============================================================================
// DAMAGE NUMBERS
// ECS-based: each damage number is its own entity that spawns on hit and
// despawns when animation completes. Supports unlimited simultaneous numbers.
// Glyphs 0-9 are pre-cached at startup to avoid WASM texture allocation issues.
// ============================================================================

/// Marker component for the glyph pre-cache entity (hidden, just warms texture)
#[derive(Component)]
pub struct GlyphCache;

/// Component for a damage number entity. Contains animation state.
#[derive(Component)]
pub struct DamageNumber {
    pub timer: f32,
    pub is_crit: bool,
    pub world_pos: Vec3, // World position where hit occurred
    pub offset: Vec2,    // Random screen offset for variety
}

pub const DAMAGE_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);
pub const CRIT_COLOR: Color = Color::srgb(1.0, 0.8, 0.2);

const DISPLAY_DURATION: f32 = 0.8;

fn setup_damage_number_pool(mut commands: Commands) {
    // Pre-cache digit glyphs at the two fixed font sizes (regular 36, crit 48).
    for size in [36.0, 48.0] {
        commands.spawn((
            GlyphCache,
            Text::new("0123456789"),
            TextFont {
                font_size: size,
                ..default()
            },
            TextColor(Color::NONE), // Invisible
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(-9999.0),
                top: Val::Px(-9999.0),
                ..default()
            },
        ));
    }
}

fn on_damage_number(on: On<HitEvent>, targets: Query<&Transform>, mut commands: Commands) {
    let event = on.event();

    let Ok(target_transform) = targets.get(event.target) else {
        return;
    };

    // World position at enemy's chest
    let world_pos = target_transform.translation + Vec3::Y * 1.0;

    let damage = event.damage as i32;
    let is_crit = event.is_crit;

    // Random offset so stacked hits don't overlap perfectly
    let mut rng = rand::rng();
    let offset = Vec2::new(
        rand::Rng::random_range(&mut rng, -20.0..20.0),
        rand::Rng::random_range(&mut rng, -10.0..10.0),
    );

    // UI text with fixed font size
    let font_size = if is_crit { 48.0 } else { 36.0 };
    commands.spawn((
        DamageNumber {
            timer: 0.0,
            is_crit,
            world_pos,
            offset,
        },
        Text::new(format!("{}", damage)),
        TextFont {
            font_size,
            ..default()
        },
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

// Animation: pop up with overshoot, hold, then rise and fade
const POP_DURATION: f32 = 0.15; // Quick pop up
const HOLD_END: f32 = 0.4; // Hold until 40%
const RISE_PIXELS: f32 = 80.0; // Total rise distance in screen pixels

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

        // Project world position to screen
        let Some(base_screen) = cam.world_to_viewport(cam_global, dmg.world_pos).ok() else {
            node.left = Val::Px(-9999.0);
            node.top = Val::Px(-9999.0);
            continue;
        };

        // Y offset animation: pop up with overshoot, then continue rising
        let y_offset = if t < POP_DURATION / DISPLAY_DURATION {
            // Pop phase: quick rise with elastic overshoot
            let pop_t = t / (POP_DURATION / DISPLAY_DURATION);
            let ease = 1.0 - (1.0 - pop_t).powi(3);
            // Overshoot: go past target then settle
            let overshoot = if pop_t > 0.6 {
                1.0 + (1.0 - pop_t) * 0.4 * ((pop_t - 0.6) / 0.4).sin() * std::f32::consts::PI
            } else {
                ease
            };
            -40.0 * overshoot // Negative because screen Y goes down
        } else {
            // After pop: continue rising smoothly
            let rise_t =
                (t - POP_DURATION / DISPLAY_DURATION) / (1.0 - POP_DURATION / DISPLAY_DURATION);
            -40.0 - (RISE_PIXELS - 40.0) * rise_t.sqrt()
        };

        // Position on screen
        node.left = Val::Px(base_screen.x + dmg.offset.x - 24.0);
        node.top = Val::Px(base_screen.y + dmg.offset.y + y_offset);

        // Fade: full during pop+hold, fade out after
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

// ============================================================================
// HEALTH BARS
// Enemy: world-tracking UI, appears on damage, fades after 3s
// Player: static HUD element, always visible
// ============================================================================

const ENEMY_BAR_WIDTH: f32 = 60.0;
const ENEMY_BAR_HEIGHT: f32 = 6.0;
const PLAYER_BAR_WIDTH: f32 = 200.0;
const PLAYER_BAR_HEIGHT: f32 = 12.0;
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

/// Enemy health bar - tracks world position
#[derive(Component)]
pub struct EnemyHealthBar {
    pub target: Entity,
    pub visible_timer: f32,
}

/// Player health bar - static HUD element
#[derive(Component)]
pub struct PlayerHealthBar;

/// Inner fill element for health bars
#[derive(Component)]
pub struct HealthBarFill;

fn on_enemy_damaged(
    on: On<DamageEvent>,
    enemies: Query<&GlobalTransform, With<Enemy>>,
    mut health_bars: Query<&mut EnemyHealthBar>,
    mut commands: Commands,
) {
    let event = on.event();

    // Check if this target already has a health bar - reset its timer
    for mut bar in health_bars.iter_mut() {
        if bar.target == event.target {
            bar.visible_timer = VISIBILITY_DURATION;
            return;
        }
    }

    // Check it's an enemy
    let Ok(_enemy_tf) = enemies.get(event.target) else {
        return;
    };

    // Spawn health bar container
    commands
        .spawn((
            DespawnOnExit(Screen::Gameplay),
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
            BorderColor::all(LIGHT_GRAY_1.with_alpha(0.6)),
            BackgroundColor(GRAY_0.with_alpha(0.7)),
            GlobalZIndex(90),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            // Fill bar
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
    on: On<DeathEvent>,
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
        // Decrement timer
        bar.visible_timer -= delta;

        if bar.visible_timer <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // Get enemy position and health
        let Ok((enemy_tf, health)) = enemies.get(bar.target) else {
            // Enemy gone, despawn bar
            commands.entity(entity).despawn();
            continue;
        };

        // Position above enemy's head
        let world_pos = enemy_tf.translation() + Vec3::Y * 2.2;
        let Some(screen_pos) = cam.world_to_viewport(cam_global, world_pos).ok() else {
            node.left = Val::Px(-9999.0);
            node.top = Val::Px(-9999.0);
            continue;
        };

        node.left = Val::Px(screen_pos.x - ENEMY_BAR_WIDTH / 2.0);
        node.top = Val::Px(screen_pos.y);

        // Fade out in last 0.5s
        let alpha = if bar.visible_timer < 0.5 {
            bar.visible_timer / 0.5
        } else {
            1.0
        };
        bg.0 = GRAY_0.with_alpha(0.7 * alpha);

        // Update fill
        let fraction = health.fraction();
        for child in children.iter() {
            if let Ok((mut fill_node, mut fill_bg)) = fills.get_mut(child) {
                fill_node.width = Val::Percent(fraction * 100.0);
                fill_bg.0 = health_color(fraction).with_alpha(alpha);
            }
        }
    }
}

/// Marker for combat stacks HUD
#[derive(Component)]
pub struct CombatStacksDisplay;

/// Individual stack indicator
#[derive(Component)]
pub struct StackIndicator(pub u32);

/// Spawns the player health bar HUD element
pub fn spawn_player_health_bar(commands: &mut Commands) {
    commands
        .spawn((
            DespawnOnExit(Screen::Gameplay),
            PlayerHealthBar,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(PLAYER_BAR_WIDTH),
                height: Val::Px(PLAYER_BAR_HEIGHT),
                left: Val::Px(20.0),
                bottom: Val::Px(20.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(4.0)),
            BorderColor::all(LIGHT_GRAY_1.with_alpha(0.6)),
            BackgroundColor(GRAY_0.with_alpha(0.7)),
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
                BorderRadius::all(Val::Px(3.0)),
                BackgroundColor(GRASS_GREEN),
            ));
        });
}

fn tick_player_health_bar(
    player: Query<&Health, With<Player>>,
    health_bars: Query<&Children, With<PlayerHealthBar>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<HealthBarFill>>,
) {
    let Ok(health) = player.single() else {
        return;
    };

    let Ok(children) = health_bars.single() else {
        return;
    };

    let fraction = health.fraction();
    for child in children.iter() {
        if let Ok((mut fill_node, mut fill_bg)) = fills.get_mut(child) {
            fill_node.width = Val::Percent(fraction * 100.0);
            fill_bg.0 = health_color(fraction);
        }
    }
}

/// Spawns the combat stacks HUD display
pub fn spawn_combat_stacks_display(commands: &mut Commands) {
    let max_stacks = 12;
    let stack_size = 12.0;
    let stack_gap = 4.0;

    commands
        .spawn((
            DespawnOnExit(Screen::Gameplay),
            CombatStacksDisplay,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(20.0),
                bottom: Val::Px(45.0), // Above health bar
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(stack_gap),
                ..default()
            },
            GlobalZIndex(90),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            for i in 0..max_stacks {
                parent.spawn((
                    StackIndicator(i),
                    Node {
                        width: Val::Px(stack_size),
                        height: Val::Px(stack_size),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BorderRadius::all(Val::Px(2.0)),
                    BorderColor::all(LIGHT_GRAY_1.with_alpha(0.4)),
                    BackgroundColor(GRAY_0.with_alpha(0.3)),
                ));
            }
        });
}

fn tick_combat_stacks_display(
    player: Query<&Stats, With<Player>>,
    mut indicators: Query<(&StackIndicator, &mut BackgroundColor)>,
) {
    let Ok(stats) = player.single() else {
        return;
    };
    // Read the stacking system's internal stat
    let stacks = stats.get(&Stat::Custom("Stacks".into())) as u32;

    for (indicator, mut bg) in indicators.iter_mut() {
        if indicator.0 < stacks {
            // Active stack - bright orange/yellow
            bg.0 = Color::srgb(1.0, 0.7, 0.2);
        } else {
            // Inactive - dim
            bg.0 = GRAY_0.with_alpha(0.3);
        }
    }
}

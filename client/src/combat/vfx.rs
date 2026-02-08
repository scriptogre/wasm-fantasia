use bevy::prelude::*;

use crate::combat::{AttackIntent, HitLanded, VFX_ARC_DEGREES, VFX_RANGE};
use crate::models::GameState;

pub fn plugin(app: &mut App) {
    app.add_observer(on_hit_flash)
        .add_observer(on_phantom_fist)
        .add_observer(on_debug_hitbox)
        .add_systems(
            Startup,
            (setup_phantom_fist_assets, setup_debug_hitbox_assets),
        )
        .add_systems(
            Update,
            (tick_hit_flash, tick_phantom_fist, tick_debug_hitbox),
        );

    #[cfg(not(target_arch = "wasm32"))]
    {
        app.add_observer(on_impact_vfx)
            .add_systems(Startup, setup_impact_assets)
            .add_systems(Update, tick_impact_vfx);
    }
}

// ── Hit Flash ───────────────────────────────────────────────────────

#[derive(Component)]
pub struct HitFlash {
    pub timer: f32,
    pub duration: f32,
    pub original_color: Color,
}

impl HitFlash {
    pub const FLASH_COLOR: Color = crate::ui::colors::NEUTRAL200;
}

fn on_hit_flash(
    on: On<HitLanded>,
    mut targets: Query<(&MeshMaterial3d<StandardMaterial>, Option<&HitFlash>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let event = on.event();

    if event.feedback.flash_duration <= 0.0 {
        return;
    }

    let Ok((mat_handle, existing_flash)) = targets.get_mut(event.target) else {
        return;
    };

    if existing_flash.is_some() {
        return;
    }

    let Some(material) = materials.get_mut(mat_handle) else {
        return;
    };

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
            if let Some(material) = materials.get_mut(mat_handle) {
                material.base_color = flash.original_color;
                material.emissive = LinearRgba::NONE;
            }
            commands.entity(entity).remove::<HitFlash>();
        }
    }
}

// ── Arc Slash (Phantom Fist) ────────────────────────────────────────

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
    let arc_mesh = create_arc_mesh(
        VFX_RANGE,
        VFX_ARC_DEGREES.to_radians(),
        0.6,
        16,
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
        let z = -angle.cos() * radius;

        positions.push([x, -half_height, z]);
        normals.push([0.0, 0.0, -1.0]);
        uvs.push([t, 0.0]);

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
    on: On<AttackIntent>,
    transforms: Query<&Transform>,
    assets: Option<Res<ArcSlashAssets>>,
    mut commands: Commands,
) {
    let Some(assets) = assets else {
        return;
    };

    let Ok(tf) = transforms.get(on.event().attacker) else {
        return;
    };

    let pos = tf.translation + Vec3::Y * 0.8;

    commands.spawn((
        ArcSlash {
            timer: 0.0,
            duration: 0.15,
            start_scale: Vec3::new(0.3, 1.0, 0.3),
        },
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material.clone()),
        Transform::from_translation(pos)
            .with_rotation(tf.rotation)
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

        let eased = 1.0 - (1.0 - t).powi(3);
        let scale = slash.start_scale.lerp(Vec3::ONE, eased);
        transform.scale = scale;
    }
}

// ── Debug Hitbox ────────────────────────────────────────────────────

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
    const DEBUG_RANGE: f32 = 3.6;
    const DEBUG_ARC: f32 = 150.0;

    let arc_mesh = create_arc_mesh(
        DEBUG_RANGE,
        DEBUG_ARC.to_radians(),
        0.1,
        24,
    );
    let mesh = meshes.add(arc_mesh);

    let material = materials.add(StandardMaterial {
        base_color: crate::ui::colors::HEALTH_RED.with_alpha(0.3),
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
    on: On<AttackIntent>,
    game_state: Res<GameState>,
    transforms: Query<&Transform>,
    assets: Option<Res<DebugHitboxAssets>>,
    mut commands: Commands,
) {
    if !game_state.debug_ui {
        return;
    }

    let Some(assets) = assets else {
        return;
    };

    let Ok(tf) = transforms.get(on.event().attacker) else {
        return;
    };

    let pos = tf.translation + Vec3::Y * 0.1;

    commands.spawn((
        DebugHitbox {
            timer: 0.0,
            duration: 0.5,
        },
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material.clone()),
        Transform::from_translation(pos).with_rotation(tf.rotation),
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

// ── Impact VFX (native only) ────────────────────────────────────────

#[derive(Resource)]
pub struct ImpactAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

#[cfg(not(target_arch = "wasm32"))]
fn setup_impact_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(0.15, 0.15, 0.5));
    let material = materials.add(StandardMaterial {
        base_color: crate::ui::colors::SAND_YELLOW,
        emissive: LinearRgba::new(15.0, 10.0, 2.0, 1.0),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        ..default()
    });
    commands.insert_resource(ImpactAssets { mesh, material });
}

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
#[derive(Component)]
pub struct ImpactBurst {
    pub timer: f32,
    pub duration: f32,
}

#[cfg(not(target_arch = "wasm32"))]
fn on_impact_vfx(
    on: On<HitLanded>,
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

    let impact_pos = target_transform.translation + Vec3::Y * 0.9;

    let num_particles = 12;
    let mut rng = rand::rng();

    for i in 0..num_particles {
        let angle = (i as f32 / num_particles as f32) * std::f32::consts::TAU;
        let vertical = rand::Rng::random_range(&mut rng, -0.3..0.5);

        let dir = Vec3::new(angle.cos(), vertical, angle.sin()).normalize();
        let speed = rand::Rng::random_range(&mut rng, 3.0..6.0);
        let duration = rand::Rng::random_range(&mut rng, 0.15..0.25);

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

#[cfg(not(target_arch = "wasm32"))]
fn tick_impact_vfx(
    time: Res<Time>,
    mut commands: Commands,
    mut particles: Query<(Entity, &mut ImpactVfx, &mut Transform), Without<ImpactBurst>>,
    mut bursts: Query<(Entity, &mut ImpactBurst, &mut Transform)>,
) {
    let dt = time.delta_secs();

    for (entity, mut vfx, mut transform) in particles.iter_mut() {
        vfx.timer += dt;
        let t = (vfx.timer / vfx.duration).min(1.0);

        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let distance = vfx.speed * vfx.timer;
        transform.translation = vfx.start_pos + vfx.direction * distance;

        let fade = 1.0 - t;
        let stretch = 1.0 + t * 2.0;
        transform.scale = Vec3::new(0.3 * fade, 0.3 * fade, 0.8 * stretch * fade);
    }

    for (entity, mut burst, mut transform) in bursts.iter_mut() {
        burst.timer += dt;
        let t = (burst.timer / burst.duration).min(1.0);

        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let ease = 1.0 - (1.0 - t).powi(3);
        let scale = 0.1 + ease * 1.5;
        let fade = 1.0 - t;
        transform.scale = Vec3::splat(scale * fade);
    }
}

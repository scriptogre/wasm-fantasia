use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;
use bevy_open_vat::prelude::OpenVatExtension;

use super::enemy::VatMeshLink;
use crate::combat::{AttackIntent, HitLanded, MeshHeight, VFX_ARC_DEGREES, VFX_RANGE};
use crate::models::Session;
use crate::player::control::{Footstep, GroundPoundImpact, JumpLaunched, LandingImpact};

type VatMaterial = ExtendedMaterial<StandardMaterial, OpenVatExtension>;

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

    app.add_observer(on_impact_vfx)
        .add_systems(Startup, setup_impact_assets)
        .add_systems(Update, tick_impact_vfx);

    app.add_observer(on_launch_shockwave)
        .add_observer(on_landing_vfx)
        .add_observer(on_ground_pound_vfx)
        .add_observer(on_footstep_dust)
        .add_systems(Startup, setup_shockwave_assets)
        .add_systems(Update, tick_shockwave_vfx);
}

// ── Hit Flash ───────────────────────────────────────────────────────

const FLASH_COLOR: Color = crate::ui::colors::NEUTRAL200;

/// Temporarily swaps an enemy's shared VAT material for a cloned copy with
/// white base_color + emissive glow. Stores the shared handle for restoration.
#[derive(Component)]
#[component(storage = "SparseSet")]
struct HitFlash {
    timer: f32,
    duration: f32,
    shared_material: Handle<VatMaterial>,
}

fn on_hit_flash(
    on: On<HitLanded>,
    vat_links: Query<&VatMeshLink>,
    vat_meshes: Query<(&MeshMaterial3d<VatMaterial>, Option<&HitFlash>)>,
    mut vat_materials: ResMut<Assets<VatMaterial>>,
    mut commands: Commands,
) {
    let event = on.event();

    if event.feedback.flash_duration <= 0.0 {
        return;
    }

    let Ok(vat_link) = vat_links.get(event.target) else {
        return;
    };
    let mesh_entity = vat_link.0;
    let Ok((mat_handle, existing_flash)) = vat_meshes.get(mesh_entity) else {
        return;
    };
    if existing_flash.is_some() {
        return;
    }

    let shared_handle = mat_handle.0.clone();
    let Some(shared_mat) = vat_materials.get(&shared_handle) else {
        return;
    };

    let mut flash_mat = shared_mat.clone();
    flash_mat.base.base_color = FLASH_COLOR;
    flash_mat.base.emissive = LinearRgba::new(2.0, 1.8, 1.5, 1.0);
    let flash_handle = vat_materials.add(flash_mat);

    // try_insert: entity may be despawned between command buffer and apply.
    commands.entity(mesh_entity).try_insert((
        MeshMaterial3d(flash_handle),
        HitFlash {
            timer: 0.0,
            duration: event.feedback.flash_duration,
            shared_material: shared_handle,
        },
    ));
}

fn tick_hit_flash(
    time: Res<Time>,
    mut flashing: Query<(Entity, &mut HitFlash, &MeshMaterial3d<VatMaterial>)>,
    mut vat_materials: ResMut<Assets<VatMaterial>>,
    mut commands: Commands,
) {
    for (entity, mut flash, mat_handle) in flashing.iter_mut() {
        flash.timer += time.delta_secs();

        if flash.timer >= flash.duration {
            vat_materials.remove(&mat_handle.0);
            commands
                .entity(entity)
                .try_insert(MeshMaterial3d(flash.shared_material.clone()))
                .try_remove::<HitFlash>();
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
#[component(storage = "SparseSet")]
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
    let arc_mesh = create_arc_mesh(VFX_RANGE, VFX_ARC_DEGREES.to_radians(), 0.6, 16);
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
#[component(storage = "SparseSet")]
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

    let arc_mesh = create_arc_mesh(DEBUG_RANGE, DEBUG_ARC.to_radians(), 0.1, 24);
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
    session: Res<Session>,
    transforms: Query<&Transform>,
    assets: Option<Res<DebugHitboxAssets>>,
    mut commands: Commands,
) {
    if !session.debug_ui {
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

// ── Impact VFX ─────────────────────────────────────────────────────

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

#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct ImpactVfx {
    pub timer: f32,
    pub duration: f32,
    pub direction: Vec3,
    pub speed: f32,
    pub start_pos: Vec3,
}

#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct ImpactBurst {
    pub timer: f32,
    pub duration: f32,
}

fn on_impact_vfx(
    on: On<HitLanded>,
    targets: Query<(&Transform, Option<&MeshHeight>)>,
    impact_assets: Option<Res<ImpactAssets>>,
    mut commands: Commands,
) {
    let Some(assets) = impact_assets else {
        return;
    };

    let event = on.event();

    let Ok((target_transform, mesh_height)) = targets.get(event.target) else {
        return;
    };

    let center_mass = mesh_height.map_or(0.9, |h| h.0 * 0.5);
    let impact_pos = target_transform.translation + Vec3::Y * center_mass;

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

// ── Launch Shockwave VFX ────────────────────────────────────────────

#[derive(Resource)]
struct ShockwaveAssets {
    ring_mesh: Handle<Mesh>,
    ring_material: Handle<StandardMaterial>,
    dust_mesh: Handle<Mesh>,
    dust_material: Handle<StandardMaterial>,
}

#[derive(Component)]
#[component(storage = "SparseSet")]
struct ShockwaveRing {
    timer: f32,
    duration: f32,
    max_scale: f32,
}

#[derive(Component)]
#[component(storage = "SparseSet")]
struct ShockwaveDust {
    timer: f32,
    duration: f32,
    direction: Vec3,
    speed: f32,
    start_pos: Vec3,
}

fn setup_shockwave_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Flat ring (washer shape) for the expanding shockwave
    let ring_mesh = meshes.add(Annulus::new(0.6, 1.0));
    let ring_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.8, 0.9, 1.0, 0.6),
        emissive: LinearRgba::new(3.0, 4.0, 8.0, 1.0),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    // Chunky dust puffs — large enough to read at game camera distance
    let dust_mesh = meshes.add(Sphere::new(0.2));
    let dust_material = materials.add(StandardMaterial {
        base_color: crate::ui::colors::SAND_YELLOW.with_alpha(0.8),
        emissive: LinearRgba::new(5.0, 3.5, 1.0, 1.0),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        ..default()
    });

    commands.insert_resource(ShockwaveAssets {
        ring_mesh,
        ring_material,
        dust_mesh,
        dust_material,
    });
}

fn on_launch_shockwave(
    on: On<JumpLaunched>,
    assets: Option<Res<ShockwaveAssets>>,
    mut commands: Commands,
) {
    let Some(assets) = assets else {
        return;
    };

    let event = on.event();
    let t = (event.charge_time / crate::player::control::MAX_CHARGE_TIME).clamp(0.0, 1.0);
    let max_scale = 0.5 + 2.5 * t;
    let pos = event.position - Vec3::Y * 0.8; // At feet level

    // Expanding ground ring
    commands.spawn((
        ShockwaveRing {
            timer: 0.0,
            duration: 0.3,
            max_scale,
        },
        Mesh3d(assets.ring_mesh.clone()),
        MeshMaterial3d(assets.ring_material.clone()),
        Transform::from_translation(pos)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(0.1)),
    ));

    let mut rng = rand::rng();

    // Dust chunks that arc outward and fall back down (gravity in tick)
    let num_particles = 10 + (6.0 * t) as usize;
    for i in 0..num_particles {
        let angle = (i as f32 / num_particles as f32) * std::f32::consts::TAU
            + rand::Rng::random_range(&mut rng, -0.2..0.2);
        let loft = rand::Rng::random_range(&mut rng, 2.0..5.0) * (0.5 + 0.5 * t);
        let dir = Vec3::new(angle.cos(), loft, angle.sin()).normalize();
        let speed = rand::Rng::random_range(&mut rng, 3.0..7.0) * (0.5 + 0.5 * t);
        let duration = rand::Rng::random_range(&mut rng, 0.3..0.5);
        let scale = rand::Rng::random_range(&mut rng, 0.5..1.2) * (0.6 + 0.4 * t);

        commands.spawn((
            ShockwaveDust {
                timer: 0.0,
                duration,
                direction: dir,
                speed,
                start_pos: pos,
            },
            Mesh3d(assets.dust_mesh.clone()),
            MeshMaterial3d(assets.dust_material.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
        ));
    }
}

fn tick_shockwave_vfx(
    time: Res<Time>,
    mut commands: Commands,
    mut rings: Query<(Entity, &mut ShockwaveRing, &mut Transform), Without<ShockwaveDust>>,
    mut dust: Query<(Entity, &mut ShockwaveDust, &mut Transform)>,
) {
    let dt = time.delta_secs();

    for (entity, mut ring, mut transform) in rings.iter_mut() {
        ring.timer += dt;
        let t = (ring.timer / ring.duration).min(1.0);

        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // Cubic ease-out for expansion
        let ease = 1.0 - (1.0 - t).powi(3);
        let scale = 0.1 + ease * ring.max_scale;
        let fade = 1.0 - t;
        // Annulus is flat — scale XZ uniformly, keep Y thin
        transform.scale = Vec3::new(scale, 0.1 * fade, scale);
    }

    for (entity, mut dust, mut transform) in dust.iter_mut() {
        dust.timer += dt;
        let t = (dust.timer / dust.duration).min(1.0);

        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // Ballistic arc: initial velocity + gravity pulls dust back down
        let time = dust.timer;
        let gravity = -12.0;
        let pos = dust.start_pos + dust.direction * dust.speed * time;
        transform.translation = Vec3::new(pos.x, pos.y + 0.5 * gravity * time * time, pos.z);

        // Don't let dust sink below spawn point (ground level)
        if transform.translation.y < dust.start_pos.y {
            transform.translation.y = dust.start_pos.y;
        }

        let fade = 1.0 - t;
        transform.scale = Vec3::splat(0.8 * fade);
    }
}

// ── Landing Impact VFX ──────────────────────────────────────────────

const LANDING_MAX_VELOCITY: f32 = 25.0;

fn on_landing_vfx(
    on: On<LandingImpact>,
    assets: Option<Res<ShockwaveAssets>>,
    mut commands: Commands,
) {
    let Some(assets) = assets else {
        return;
    };

    let event = on.event();
    let t = ((event.velocity_y - 3.0) / (LANDING_MAX_VELOCITY - 3.0)).clamp(0.0, 1.0);
    let pos = event.position - Vec3::Y * 0.8;

    // Landing ground ring
    let max_scale = 0.5 + 3.0 * t;
    commands.spawn((
        ShockwaveRing {
            timer: 0.0,
            duration: 0.35,
            max_scale,
        },
        Mesh3d(assets.ring_mesh.clone()),
        MeshMaterial3d(assets.ring_material.clone()),
        Transform::from_translation(pos)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(0.1)),
    ));

    let mut rng = rand::rng();

    // Debris chunks that arc outward and fall back (gravity in tick)
    let num_particles = 12 + (10.0 * t) as usize;
    for i in 0..num_particles {
        let angle = (i as f32 / num_particles as f32) * std::f32::consts::TAU
            + rand::Rng::random_range(&mut rng, -0.2..0.2);
        let loft = rand::Rng::random_range(&mut rng, 2.0..6.0) * (0.4 + 0.6 * t);
        let dir = Vec3::new(angle.cos(), loft, angle.sin()).normalize();
        let speed = rand::Rng::random_range(&mut rng, 3.0..8.0) * (0.4 + 0.6 * t);
        let duration = rand::Rng::random_range(&mut rng, 0.35..0.55);
        let scale = rand::Rng::random_range(&mut rng, 0.5..1.3) * (0.5 + 0.6 * t);

        commands.spawn((
            ShockwaveDust {
                timer: 0.0,
                duration,
                direction: dir,
                speed,
                start_pos: pos,
            },
            Mesh3d(assets.dust_mesh.clone()),
            MeshMaterial3d(assets.dust_material.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
        ));
    }
}

// ── Ground Pound VFX ────────────────────────────────────────────────

fn on_ground_pound_vfx(
    on: On<GroundPoundImpact>,
    assets: Option<Res<ShockwaveAssets>>,
    mut commands: Commands,
) {
    let Some(assets) = assets else {
        return;
    };

    let event = on.event();
    let pos = event.position - Vec3::Y * 0.8;

    // Large expanding ground ring
    commands.spawn((
        ShockwaveRing {
            timer: 0.0,
            duration: 0.4,
            max_scale: 4.0,
        },
        Mesh3d(assets.ring_mesh.clone()),
        MeshMaterial3d(assets.ring_material.clone()),
        Transform::from_translation(pos)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(0.1)),
    ));

    let mut rng = rand::rng();

    // Dense debris cloud — arcing outward with strong loft (gravity in tick)
    for i in 0..24 {
        let angle = (i as f32 / 24.0) * std::f32::consts::TAU
            + rand::Rng::random_range(&mut rng, -0.15..0.15);
        let loft = rand::Rng::random_range(&mut rng, 3.0..7.0);
        let dir = Vec3::new(angle.cos(), loft, angle.sin()).normalize();
        let speed = rand::Rng::random_range(&mut rng, 4.0..10.0);
        let duration = rand::Rng::random_range(&mut rng, 0.4..0.6);
        let scale = rand::Rng::random_range(&mut rng, 0.6..1.4);

        commands.spawn((
            ShockwaveDust {
                timer: 0.0,
                duration,
                direction: dir,
                speed,
                start_pos: pos,
            },
            Mesh3d(assets.dust_mesh.clone()),
            MeshMaterial3d(assets.dust_material.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
        ));
    }
}

// ── Footstep Dust ───────────────────────────────────────────────────

fn on_footstep_dust(
    on: On<Footstep>,
    assets: Option<Res<ShockwaveAssets>>,
    mut commands: Commands,
) {
    let Some(assets) = assets else {
        return;
    };

    let event = on.event();
    let pos = event.position - Vec3::Y * 0.8;
    let mut rng = rand::rng();

    for _ in 0..5 {
        let angle = rand::Rng::random_range(&mut rng, 0.0..std::f32::consts::TAU);
        let vert = rand::Rng::random_range(&mut rng, 0.1..0.3);
        let dir = Vec3::new(angle.cos(), vert, angle.sin()).normalize();
        let speed = rand::Rng::random_range(&mut rng, 1.5..3.0);
        let duration = rand::Rng::random_range(&mut rng, 0.2..0.35);

        commands.spawn((
            ShockwaveDust {
                timer: 0.0,
                duration,
                direction: dir,
                speed,
                start_pos: pos,
            },
            Mesh3d(assets.dust_mesh.clone()),
            MeshMaterial3d(assets.dust_material.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(0.5)),
        ));
    }
}

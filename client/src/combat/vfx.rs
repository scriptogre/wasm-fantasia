use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;
use bevy_hanabi::prelude::{
    self as hanabi, AccelModifier, ColorBlendMask, ColorBlendMode, ColorOverLifetimeModifier,
    EffectAsset, EffectMaterial, ExprWriter, ImageSampleMapping, LinearDragModifier, OrientMode,
    OrientModifier, ParticleEffect, ParticleTextureModifier, SetAttributeModifier,
    SetPositionSphereModifier, SetVelocitySphereModifier, ShapeDimension, SizeOverLifetimeModifier,
    SpawnerSettings,
};
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
        .add_observer(on_jump_vfx)
        .add_observer(on_landing_vfx)
        .add_observer(on_ground_pound_vfx)
        .add_observer(on_footstep_dust)
        .add_systems(Startup, setup_particle_effects)
        .add_systems(Update, tick_debris_chunks);
}

// ── Hit Flash ───────────────────────────────────────────────────────

/// Temporarily swaps an enemy's shared VAT material for a pre-allocated flash
/// copy with white base_color + emissive glow. Stores the shared handle for restoration.
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
    vat_state: Option<Res<super::enemy::VatEnemyState>>,
    mut commands: Commands,
) {
    let event = on.event();

    if event.feedback.flash_duration <= 0.0 {
        return;
    }

    let Some(vat_state) = vat_state else {
        return;
    };

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

    // Use pre-allocated flash material — no per-hit material clone.
    commands.entity(mesh_entity).try_insert((
        MeshMaterial3d(vat_state.flash_material.clone()),
        HitFlash {
            timer: 0.0,
            duration: event.feedback.flash_duration,
            shared_material: shared_handle,
        },
    ));
}

fn tick_hit_flash(
    time: Res<Time>,
    mut flashing: Query<(Entity, &mut HitFlash)>,
    mut commands: Commands,
) {
    for (entity, mut flash) in flashing.iter_mut() {
        flash.timer += time.delta_secs();

        if flash.timer >= flash.duration {
            // Restore original material — don't remove the flash material asset,
            // it's shared across all enemies (pre-allocated in VatEnemyState).
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
    use game_core::combat::defaults;
    let debug_range = defaults::ATTACK_RANGE;
    let debug_arc = defaults::ATTACK_ARC;

    let arc_mesh = create_arc_mesh(debug_range, debug_arc.to_radians(), 0.1, 24);
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

// ── GPU Particle Effects (bevy_hanabi) ──────────────────────────────

#[derive(Resource)]
struct ParticleEffects {
    dust_burst: Handle<EffectAsset>,
    jump_dust_cloud: Handle<EffectAsset>,
    landing_impact: Handle<EffectAsset>,
    ground_pound: Handle<EffectAsset>,
    hit_spark: Handle<EffectAsset>,
    smoke_texture: Handle<Image>,
}

#[derive(Resource)]
struct DebrisAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Component)]
#[component(storage = "SparseSet")]
struct DebrisChunk {
    timer: f32,
    duration: f32,
    velocity: Vec3,
    angular_velocity: Vec3,
    start_pos: Vec3,
}

fn setup_particle_effects(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let smoke_texture: Handle<Image> = asset_server.load("textures/smoke.png");

    let dust_burst = effects.add(make_dust_burst());
    let jump_dust_cloud = effects.add(make_jump_dust_cloud());
    let landing_impact = effects.add(make_landing_impact());
    let ground_pound = effects.add(make_ground_pound());
    let hit_spark = effects.add(make_hit_spark());

    commands.insert_resource(ParticleEffects {
        dust_burst,
        jump_dust_cloud,
        landing_impact,
        ground_pound,
        hit_spark,
        smoke_texture,
    });

    // Debris chunk assets for jump launch
    let debris_mesh = meshes.add(Cuboid::new(0.15, 0.1, 0.12));
    let debris_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.13, 0.1),
        perceptual_roughness: 0.9,
        ..default()
    });
    commands.insert_resource(DebrisAssets {
        mesh: debris_mesh,
        material: debris_material,
    });
}

fn make_dust_burst() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(0.3).expr(),
        dimension: ShapeDimension::Volume,
    };
    let init_vel = SetVelocitySphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        speed: writer.lit(1.5).uniform(writer.lit(3.0)).expr(),
    };
    let init_lifetime = SetAttributeModifier::new(
        hanabi::Attribute::LIFETIME,
        writer.lit(0.2).uniform(writer.lit(0.4)).expr(),
    );
    let init_age = SetAttributeModifier::new(hanabi::Attribute::AGE, writer.lit(0.0).expr());

    let drag = LinearDragModifier::new(writer.lit(6.0).expr());
    let gravity = AccelModifier::new(writer.lit(Vec3::new(0., -5.0, 0.)).expr());

    let mut color_grad = hanabi::Gradient::new();
    color_grad.add_key(0.0, Vec4::new(0.25, 0.2, 0.15, 0.5));
    color_grad.add_key(0.5, Vec4::new(0.18, 0.15, 0.12, 0.25));
    color_grad.add_key(1.0, Vec4::new(0.1, 0.08, 0.06, 0.0));

    let mut size_grad = hanabi::Gradient::new();
    size_grad.add_key(0.0, Vec3::splat(0.08));
    size_grad.add_key(0.3, Vec3::splat(0.12));
    size_grad.add_key(1.0, Vec3::splat(0.02));

    EffectAsset::new(256, SpawnerSettings::once(20.0.into()), writer.finish())
        .with_name("dust_burst")
        .init(init_pos)
        .init(init_vel)
        .init(init_lifetime)
        .init(init_age)
        .update(drag)
        .update(gravity)
        .render(ColorOverLifetimeModifier {
            gradient: color_grad,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier {
            gradient: size_grad,
            screen_space_size: false,
        })
}

/// Heavy dark dust/smoke cloud — "extreme ghost layer" approach.
/// Ultra-low alpha so individual particles are nearly invisible;
/// only visible where many overlap, creating a cohesive cloud that hides edges.
/// Massive expansion (2.0 → 6.5) + long lifetime forces textures to wash out.
pub fn make_jump_dust_cloud() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_age = SetAttributeModifier::new(hanabi::Attribute::AGE, writer.lit(0.).expr());

    let lifetime = writer.lit(1.8).uniform(writer.lit(3.0)).expr();
    let init_lifetime = SetAttributeModifier::new(hanabi::Attribute::LIFETIME, lifetime);

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(1.5).expr(),
        dimension: ShapeDimension::Volume,
    };

    let init_vel = SetVelocitySphereModifier {
        center: writer.lit(Vec3::new(0.0, 0.2, 0.0)).expr(),
        speed: writer.lit(12.0).uniform(writer.lit(22.0)).expr(),
    };

    let texture_slot = writer.lit(0u32).expr();
    let random_rotation = writer
        .lit(0.0)
        .uniform(writer.lit(std::f32::consts::TAU))
        .expr();

    let random_size = writer.lit(0.5).uniform(writer.lit(2.0)).expr();
    let init_size = SetAttributeModifier::new(hanabi::Attribute::SIZE, random_size);

    let drag = LinearDragModifier::new(writer.lit(6.0).expr());
    let gravity = AccelModifier::new(writer.lit(Vec3::new(0.0, -0.5, 0.0)).expr());

    let mut color_grad = hanabi::Gradient::new();
    color_grad.add_key(0.0, Vec4::new(0.1, 0.09, 0.08, 0.15));
    color_grad.add_key(0.2, Vec4::new(0.08, 0.07, 0.06, 0.12));
    color_grad.add_key(1.0, Vec4::new(0.0, 0.0, 0.0, 0.0));

    let mut size_grad = hanabi::Gradient::new();
    size_grad.add_key(0.0, Vec3::splat(2.0));
    size_grad.add_key(1.0, Vec3::splat(6.5));

    let mut module = writer.finish();
    module.add_texture_slot("smoke");

    EffectAsset::new(256, SpawnerSettings::once(60.0.into()), module)
        .with_name("jump_dust_cloud")
        .with_alpha_mode(hanabi::AlphaMode::Blend)
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .init(init_size)
        .update(drag)
        .update(gravity)
        .render(ParticleTextureModifier {
            texture_slot,
            sample_mapping: ImageSampleMapping::Modulate,
        })
        .render(OrientModifier {
            mode: OrientMode::FaceCameraPosition,
            rotation: Some(random_rotation),
        })
        .render(ColorOverLifetimeModifier {
            gradient: color_grad,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier {
            gradient: size_grad,
            screen_space_size: false,
        })
}

fn make_landing_impact() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(0.5).expr(),
        dimension: ShapeDimension::Surface,
    };
    let init_vel = SetVelocitySphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        speed: writer.lit(5.0).uniform(writer.lit(10.0)).expr(),
    };
    let init_lifetime = SetAttributeModifier::new(
        hanabi::Attribute::LIFETIME,
        writer.lit(0.35).uniform(writer.lit(0.6)).expr(),
    );
    let init_age = SetAttributeModifier::new(hanabi::Attribute::AGE, writer.lit(0.0).expr());

    let drag = LinearDragModifier::new(writer.lit(3.5).expr());
    let gravity = AccelModifier::new(writer.lit(Vec3::new(0., -14.0, 0.)).expr());

    let mut color_grad = hanabi::Gradient::new();
    color_grad.add_key(0.0, Vec4::new(0.3, 0.25, 0.18, 0.6));
    color_grad.add_key(0.3, Vec4::new(0.2, 0.16, 0.12, 0.35));
    color_grad.add_key(1.0, Vec4::new(0.1, 0.08, 0.06, 0.0));

    let mut size_grad = hanabi::Gradient::new();
    size_grad.add_key(0.0, Vec3::splat(0.12));
    size_grad.add_key(0.4, Vec3::splat(0.18));
    size_grad.add_key(1.0, Vec3::splat(0.0));

    EffectAsset::new(1024, SpawnerSettings::once(80.0.into()), writer.finish())
        .with_name("landing_impact")
        .init(init_pos)
        .init(init_vel)
        .init(init_lifetime)
        .init(init_age)
        .update(drag)
        .update(gravity)
        .render(ColorOverLifetimeModifier {
            gradient: color_grad,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier {
            gradient: size_grad,
            screen_space_size: false,
        })
}

fn make_ground_pound() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(0.6).expr(),
        dimension: ShapeDimension::Surface,
    };
    let init_vel = SetVelocitySphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        speed: writer.lit(6.0).uniform(writer.lit(14.0)).expr(),
    };
    let init_lifetime = SetAttributeModifier::new(
        hanabi::Attribute::LIFETIME,
        writer.lit(0.4).uniform(writer.lit(0.7)).expr(),
    );
    let init_age = SetAttributeModifier::new(hanabi::Attribute::AGE, writer.lit(0.0).expr());

    let drag = LinearDragModifier::new(writer.lit(3.0).expr());
    let gravity = AccelModifier::new(writer.lit(Vec3::new(0., -10.0, 0.)).expr());

    let mut color_grad = hanabi::Gradient::new();
    color_grad.add_key(0.0, Vec4::new(3.0, 2.0, 0.8, 1.0));
    color_grad.add_key(0.3, Vec4::new(1.5, 0.8, 0.2, 0.9));
    color_grad.add_key(0.7, Vec4::new(0.5, 0.3, 0.1, 0.5));
    color_grad.add_key(1.0, Vec4::new(0.15, 0.1, 0.05, 0.0));

    let mut size_grad = hanabi::Gradient::new();
    size_grad.add_key(0.0, Vec3::splat(0.15));
    size_grad.add_key(0.3, Vec3::splat(0.25));
    size_grad.add_key(1.0, Vec3::splat(0.0));

    EffectAsset::new(2048, SpawnerSettings::once(200.0.into()), writer.finish())
        .with_name("ground_pound")
        .init(init_pos)
        .init(init_vel)
        .init(init_lifetime)
        .init(init_age)
        .update(drag)
        .update(gravity)
        .render(ColorOverLifetimeModifier {
            gradient: color_grad,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier {
            gradient: size_grad,
            screen_space_size: false,
        })
}

fn make_hit_spark() -> EffectAsset {
    let writer = ExprWriter::new();

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(0.15).expr(),
        dimension: ShapeDimension::Volume,
    };
    let init_vel = SetVelocitySphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        speed: writer.lit(5.0).uniform(writer.lit(12.0)).expr(),
    };
    let init_lifetime = SetAttributeModifier::new(
        hanabi::Attribute::LIFETIME,
        writer.lit(0.1).uniform(writer.lit(0.25)).expr(),
    );
    let init_age = SetAttributeModifier::new(hanabi::Attribute::AGE, writer.lit(0.0).expr());

    let drag = LinearDragModifier::new(writer.lit(8.0).expr());

    let mut color_grad = hanabi::Gradient::new();
    color_grad.add_key(0.0, Vec4::new(6.0, 5.0, 2.0, 1.0));
    color_grad.add_key(0.3, Vec4::new(4.0, 2.0, 0.5, 0.9));
    color_grad.add_key(1.0, Vec4::new(1.0, 0.2, 0.0, 0.0));

    let mut size_grad = hanabi::Gradient::new();
    size_grad.add_key(0.0, Vec3::splat(0.06));
    size_grad.add_key(0.5, Vec3::splat(0.03));
    size_grad.add_key(1.0, Vec3::splat(0.0));

    EffectAsset::new(512, SpawnerSettings::once(30.0.into()), writer.finish())
        .with_name("hit_spark")
        .init(init_pos)
        .init(init_vel)
        .init(init_lifetime)
        .init(init_age)
        .update(drag)
        .render(ColorOverLifetimeModifier {
            gradient: color_grad,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(SizeOverLifetimeModifier {
            gradient: size_grad,
            screen_space_size: false,
        })
}

// ── Debris Chunks ───────────────────────────────────────────────────

fn tick_debris_chunks(
    time: Res<Time>,
    mut commands: Commands,
    mut chunks: Query<(Entity, &mut DebrisChunk, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let gravity = Vec3::new(0.0, -35.0, 0.0);

    for (entity, mut chunk, mut transform) in chunks.iter_mut() {
        chunk.timer += dt;

        if chunk.timer >= chunk.duration || transform.translation.y < chunk.start_pos.y - 0.5 {
            commands.entity(entity).despawn();
            continue;
        }

        chunk.velocity += gravity * dt;
        transform.translation += chunk.velocity * dt;

        let delta_rotation = Quat::from_scaled_axis(chunk.angular_velocity * dt);
        transform.rotation = delta_rotation * transform.rotation;
    }
}

// ── Observer Handlers ───────────────────────────────────────────────

fn on_jump_vfx(
    on: On<JumpLaunched>,
    effects: Option<Res<ParticleEffects>>,
    debris_assets: Option<Res<DebrisAssets>>,
    mut commands: Commands,
) {
    let event = on.event();
    let pos = event.position - Vec3::Y * 0.8;
    let ground_transform = Transform::from_translation(pos);

    if let Some(effects) = effects {
        commands.spawn((
            ParticleEffect::new(effects.jump_dust_cloud.clone()),
            EffectMaterial {
                images: vec![effects.smoke_texture.clone()],
            },
            ground_transform,
        ));
    }

    // Debris chunks (mesh entities with ballistic arcs + spin)
    if let Some(debris_assets) = debris_assets {
        let mut rng = rand::rng();
        for _ in 0..8 {
            let angle = rand::Rng::random_range(&mut rng, 0.0..std::f32::consts::TAU);
            let horizontal_speed = rand::Rng::random_range(&mut rng, 4.0..10.0);
            let upward_speed = rand::Rng::random_range(&mut rng, 8.0..16.0);
            let velocity = Vec3::new(
                angle.cos() * horizontal_speed,
                upward_speed,
                angle.sin() * horizontal_speed,
            );
            let angular_velocity = Vec3::new(
                rand::Rng::random_range(&mut rng, -10.0..10.0),
                rand::Rng::random_range(&mut rng, -10.0..10.0),
                rand::Rng::random_range(&mut rng, -10.0..10.0),
            );
            let scale = rand::Rng::random_range(&mut rng, 0.5..1.2);

            commands.spawn((
                DebrisChunk {
                    timer: 0.0,
                    duration: 2.0,
                    velocity,
                    angular_velocity,
                    start_pos: pos,
                },
                Mesh3d(debris_assets.mesh.clone()),
                MeshMaterial3d(debris_assets.material.clone()),
                ground_transform.with_scale(Vec3::splat(scale)),
            ));
        }
    }
}

const LANDING_MAX_VELOCITY: f32 = 25.0;

fn on_landing_vfx(
    on: On<LandingImpact>,
    effects: Option<Res<ParticleEffects>>,
    mut commands: Commands,
) {
    let event = on.event();
    let t = ((event.velocity_y - 3.0) / (LANDING_MAX_VELOCITY - 3.0)).clamp(0.0, 1.0);
    let pos = event.position - Vec3::Y * 0.8;

    if let Some(effects) = effects {
        let scale = 0.6 + 0.8 * t;
        commands.spawn((
            ParticleEffect::new(effects.landing_impact.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
        ));
    }
}

fn on_ground_pound_vfx(
    on: On<GroundPoundImpact>,
    effects: Option<Res<ParticleEffects>>,
    mut commands: Commands,
) {
    let event = on.event();
    let pos = event.position - Vec3::Y * 0.8;

    if let Some(effects) = effects {
        commands.spawn((
            ParticleEffect::new(effects.ground_pound.clone()),
            Transform::from_translation(pos),
        ));
    }
}

fn on_footstep_dust(
    on: On<Footstep>,
    effects: Option<Res<ParticleEffects>>,
    mut commands: Commands,
) {
    let Some(effects) = effects else {
        return;
    };

    let event = on.event();
    let pos = event.position - Vec3::Y * 0.8;

    let scale = if event.is_sprinting { 1.2 } else { 0.6 };

    commands.spawn((
        ParticleEffect::new(effects.dust_burst.clone()),
        Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
    ));
}

fn on_impact_vfx(
    on: On<HitLanded>,
    targets: Query<(&Transform, Option<&MeshHeight>)>,
    effects: Option<Res<ParticleEffects>>,
    mut commands: Commands,
) {
    let Some(effects) = effects else {
        return;
    };

    let event = on.event();

    let Ok((target_transform, mesh_height)) = targets.get(event.target) else {
        return;
    };

    let center_mass = mesh_height.map_or(0.9, |h| h.0 * 0.5);
    let impact_pos = target_transform.translation + Vec3::Y * center_mass;

    commands.spawn((
        ParticleEffect::new(effects.hit_spark.clone()),
        Transform::from_translation(impact_pos),
    ));
}

use super::*;
use crate::asset_loading::Models;
use crate::models::{ClearEnemies, SpawnEnemy};
use bevy::gltf::GltfMesh;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::pbr::ExtendedMaterial;
use bevy::render::storage::ShaderStorageBuffer;
use bevy_enhanced_input::prelude::Start;
use bevy_open_vat::data::VatInstanceData;
use bevy_open_vat::prelude::*;

/// bevy_open_vat's material type with StandardMaterial base.
type VatMaterial = ExtendedMaterial<StandardMaterial, OpenVatExtension>;

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_enemy_in_front)
        .add_observer(clear_all_enemies)
        .add_observer(on_enemy_added)
        .add_systems(
            Update,
            (
                initialize_vat_enemy_resources
                    .run_if(not(resource_exists::<VatEnemyState>).and(in_state(Screen::Gameplay))),
                attach_vat_to_pending_enemies
                    .run_if(resource_exists::<VatEnemyState>.and(in_state(Screen::Gameplay))),
                animate_enemies
                    .in_set(PostPhysicsAppSystems::PlayAnimations)
                    .run_if(in_state(Screen::Gameplay)),
            ),
        );
}

// =============================================================================
// VAT resources — shared across all enemy instances
// =============================================================================

/// Shared VAT rendering resources for all enemy instances, created once on
/// first gameplay frame when all assets are loaded.
#[derive(Resource)]
pub(crate) struct VatEnemyState {
    pub(crate) material: Handle<VatMaterial>,
    /// Pre-allocated flash material — same VAT setup but white + emissive.
    /// Shared across all enemies to avoid per-hit material clones.
    pub flash_material: Handle<VatMaterial>,
    /// The actual mesh handle extracted from the GLTF, so we can spawn
    /// Mesh3d directly instead of going through SceneRoot.
    mesh: Handle<Mesh>,
}

/// Links an enemy entity to the child mesh entity that holds the
/// `VatAnimationController`, so `animate_enemies` can update the clip.
#[derive(Component)]
pub(crate) struct VatMeshLink(pub Entity);

/// Marker for enemies that spawned before VatEnemyState was ready.
/// The `attach_vat_to_pending_enemies` system picks these up.
#[derive(Component)]
#[component(storage = "SparseSet")]
struct PendingVatSetup;

fn initialize_vat_enemy_resources(
    models: Res<Models>,
    images: Res<Assets<Image>>,
    remap_infos: Res<Assets<RemapInfo>>,
    gltf_assets: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut vat_materials: ResMut<Assets<VatMaterial>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut commands: Commands,
) {
    let Some(remap_info) = remap_infos.get(&models.enemy_remap_info) else {
        return;
    };
    let Some(image) = images.get(&models.enemy_vat_texture) else {
        return;
    };
    let Some(gltf) = gltf_assets.get(&models.enemy_scene) else {
        return;
    };
    let Some(gltf_mesh) = gltf_meshes.get(&gltf.meshes[0]) else {
        return;
    };

    let mesh = gltf_mesh.primitives[0].mesh.clone();
    let y_resolution = image.texture_descriptor.size.height as f32;

    // Seed with one zeroed entry so the GPU buffer has non-zero arrayLength.
    // bevy_open_vat's update_instance_data system overwrites this every frame.
    let mut buffer = ShaderStorageBuffer::default();
    buffer.set_data(vec![VatInstanceData::default()]);
    let buffer = buffers.add(buffer);

    let material = vat_materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.816, 0.125, 0.125),
            // Force forward rendering. The project uses deferred rendering by
            // default, but bevy_open_vat overrides vertex_shader() (forward) and
            // prepass_vertex_shader() (prepass). In deferred mode, opaque meshes
            // render through the G-buffer prepass — which DOES use the prepass
            // vertex shader. However, bevy_open_vat's prepass shader has its own
            // Vertex struct that can conflict with deferred-specific shader_defs
            // (NORMAL_PREPASS_OR_DEFERRED_PREPASS). Forward rendering avoids this
            // issue entirely.
            opaque_render_method: bevy::pbr::OpaqueRendererMethod::Forward,
            ..default()
        },
        extension: OpenVatExtension {
            vat_texture: models.enemy_vat_texture.clone(),
            min_pos: remap_info.os_remap.min.into(),
            frame_count: remap_info.os_remap.frames,
            max_pos: remap_info.os_remap.max.into(),
            y_resolution,
            instance: buffer.clone(),
        },
    });

    let flash_material = vat_materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: crate::ui::colors::NEUTRAL200,
            emissive: LinearRgba::new(2.0, 1.8, 1.5, 1.0),
            opaque_render_method: bevy::pbr::OpaqueRendererMethod::Forward,
            ..default()
        },
        extension: OpenVatExtension {
            vat_texture: models.enemy_vat_texture.clone(),
            min_pos: remap_info.os_remap.min.into(),
            frame_count: remap_info.os_remap.frames,
            max_pos: remap_info.os_remap.max.into(),
            y_resolution,
            instance: buffer.clone(),
        },
    });

    commands.insert_resource(VatEnemyState {
        material,
        flash_material,
        mesh,
    });
}

// =============================================================================
// Spawn trigger (E key / server request)
// =============================================================================

/// Spawn a pack of enemies via server reducer.
/// All game modes go through SpacetimeDB when connected.
fn spawn_enemy_in_front(
    _on: On<Start<SpawnEnemy>>,
    player: Query<&Transform, With<Player>>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };

    let forward = player_transform.forward();
    let pos = player_transform.translation;

    if let Some(conn) = conn {
        use spacetimedb_sdk::DbContext;
        if conn.conn.is_active() {
            crate::networking::combat::server_spawn_enemies(&conn, pos, forward.as_vec3());
            debug!("Requested enemies from server");
            return;
        }
    }

    warn!("No server connection — cannot spawn enemies");
}

/// Delete all enemies in the current world via server reducer.
fn clear_all_enemies(
    _on: On<Start<ClearEnemies>>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
) {
    if let Some(conn) = conn {
        use spacetimedb_sdk::DbContext;
        if conn.conn.is_active() {
            crate::networking::combat::server_clear_enemies(&conn);
            info!("Requested enemy clear from server");
            return;
        }
    }

    warn!("No server connection — cannot clear enemies");
}

// =============================================================================
// On<Add, Enemy> — attach VAT mesh directly (flat hierarchy)
// =============================================================================

/// Visual offset baked into the mesh child entity's transform.
const MESH_OFFSET: Vec3 = Vec3::new(0.0, -0.85, 0.0);
const MESH_SCALE: f32 = 1.25;

fn on_enemy_added(
    on: On<Add, Enemy>,
    vat_state: Option<Res<VatEnemyState>>,
    models: Res<Models>,
    mut commands: Commands,
) {
    let entity = on.entity;

    // Remove capsule mesh if present (reconciler may have added it)
    commands
        .entity(entity)
        .remove::<Mesh3d>()
        .remove::<MeshMaterial3d<StandardMaterial>>();

    // No physics components — attack system uses manual distance checks,
    // and server handles all movement/knockback. Avian3d was costing ~8ms
    // for 5000 kinematic sensors.
    commands.entity(entity).insert((
        EnemyBehavior::default(),
        InheritedVisibility::default(),
    ));

    if let Some(vat_state) = vat_state {
        // VatEnemyState is ready — spawn mesh child directly (flat hierarchy)
        spawn_vat_mesh_child(&mut commands, entity, &vat_state, &models);
    } else {
        // Assets not loaded yet — mark for later setup
        commands.entity(entity).insert(PendingVatSetup);
    }
}

/// Catch-up system: attaches VAT mesh to enemies that were added before
/// VatEnemyState was ready.
fn attach_vat_to_pending_enemies(
    pending: Query<Entity, With<PendingVatSetup>>,
    vat_state: Res<VatEnemyState>,
    models: Res<Models>,
    mut commands: Commands,
) {
    for entity in &pending {
        commands.entity(entity).remove::<PendingVatSetup>();
        spawn_vat_mesh_child(&mut commands, entity, &vat_state, &models);
    }
}

/// Spawns a single mesh child entity with VAT material + controller.
/// No SceneRoot, no intermediate entities — just Enemy → Mesh.
fn spawn_vat_mesh_child(
    commands: &mut Commands,
    enemy_entity: Entity,
    vat_state: &VatEnemyState,
    models: &Models,
) {
    let mesh_entity = commands
        .spawn((
            Transform::from_translation(MESH_OFFSET)
                .with_scale(Vec3::splat(MESH_SCALE))
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            Mesh3d(vat_state.mesh.clone()),
            MeshMaterial3d(vat_state.material.clone()),
            VatAnimationController {
                remap_info: models.enemy_remap_info.clone(),
                current_clip: "Zombie_Idle_Loop".to_string(),
                speed: 1.0,
                is_playing: true,
                start_time: 0.0,
                offset: 0.0,
            },
            NotShadowCaster,
            NotShadowReceiver,
        ))
        .id();

    commands
        .entity(enemy_entity)
        .add_children(&[mesh_entity])
        .insert(VatMeshLink(mesh_entity));
}

// =============================================================================
// Animation driver — maps EnemyBehavior to VAT clip names
// =============================================================================

fn animate_enemies(
    enemies: Query<(&EnemyBehavior, &VatMeshLink), Changed<EnemyBehavior>>,
    mut controllers: Query<&mut VatAnimationController>,
    time: Res<Time>,
) {
    for (behavior, vat_link) in &enemies {
        let Ok(mut controller) = controllers.get_mut(vat_link.0) else {
            continue;
        };

        let clip_name = match behavior {
            EnemyBehavior::Idle => "Zombie_Idle_Loop",
            EnemyBehavior::Chase => "Zombie_Walk_Fwd_Loop",
            EnemyBehavior::Attack => "Zombie_Scratch",
        };

        if controller.current_clip != clip_name {
            controller.current_clip = clip_name.to_string();
            controller.start_time = time.elapsed_secs();
        }
    }
}

use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::GltfMesh;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::Indices;
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
use bevy_open_vat::data::VatInstanceData;
use bevy_open_vat::prelude::*;

use crate::asset_loading::Models;

use super::mesh_simplify::simplify_indices;

/// bevy_open_vat's material type with StandardMaterial base.
type VatMaterial = ExtendedMaterial<StandardMaterial, OpenVatExtension>;

/// Visual offset baked into the mesh child entity's transform.
const MESH_OFFSET: Vec3 = Vec3::new(0.0, -0.85, 0.0);
const MESH_SCALE: f32 = 1.25;

/// VisibilityRange boundaries for LOD tiers.
/// LOD0 (full detail): 0–30m, LOD1 (simplified): 30–155m.
/// Hard cuts (equal start..end) avoid rendering both simultaneously.
const LOD_BOUNDARY: f32 = 30.0;
const LOD1_END: f32 = 155.0;

/// VisibilityRange per LOD tier. Index must match `VatEnemyState::meshes`.
const LOD_VISIBILITY_RANGES: &[VisibilityRange] = &[
    // LOD0: full mesh, 0–30m
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: LOD_BOUNDARY..LOD_BOUNDARY,
        use_aabb: false,
    },
    // LOD1: simplified mesh, 30–155m
    VisibilityRange {
        start_margin: LOD_BOUNDARY..LOD_BOUNDARY,
        end_margin: LOD1_END..LOD1_END,
        use_aabb: false,
    },
];

/// Shared VAT rendering resources for all enemy instances, created once on
/// first gameplay frame when all assets are loaded.
#[derive(Resource)]
pub(crate) struct VatEnemyState {
    pub(crate) material: Handle<VatMaterial>,
    /// Pre-allocated flash material — same VAT setup but white + emissive.
    /// Shared across all enemies to avoid per-hit material clones.
    pub flash_material: Handle<VatMaterial>,
    /// LOD mesh handles: [lod0 (full), lod1 (simplified)].
    meshes: Vec<Handle<Mesh>>,
    /// UV_B attribute extracted from LOD0 mesh — maps vertex_id to VAT texture column.
    /// Uploaded once to GPU for the compute shader.
    pub(crate) vertex_uvs: Handle<ShaderStorageBuffer>,
    /// Number of vertices in the mesh (needed by compute dispatch and vertex shader).
    pub(crate) vertex_count: u32,
}

/// Links an enemy entity to the child mesh entities (one per LOD) that hold
/// `VatAnimationController`, so `animate_enemies` can update the clip.
#[derive(Component)]
pub(crate) struct VatMeshLink(pub Vec<Entity>);

/// Initializes shared VAT rendering resources for enemies: materials, meshes,
/// LOD generation. Run once when all assets are loaded.
pub(crate) fn initialize_vat_enemy_resources(
    models: Res<Models>,
    images: Res<Assets<Image>>,
    remap_infos: Res<Assets<RemapInfo>>,
    gltf_assets: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut vat_materials: ResMut<Assets<VatMaterial>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
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

    let mesh_lod0 = gltf_mesh.primitives[0].mesh.clone();
    let mesh_lod1 = generate_lod_mesh(&mesh_lod0, &mut meshes);
    let y_resolution = image.texture_descriptor.size.height as f32;

    // Extract UV_B for compute pre-skinning
    let source_mesh = meshes.get(&mesh_lod0).expect("LOD0 mesh must exist");
    let vertex_count = source_mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .map(|attr| match attr {
            bevy::mesh::VertexAttributeValues::Float32x3(v) => v.len() as u32,
            _ => 0,
        })
        .unwrap_or(0);

    let uv_b_data: Vec<[f32; 2]> = source_mesh
        .attribute(Mesh::ATTRIBUTE_UV_1)
        .and_then(|attr| match attr {
            bevy::mesh::VertexAttributeValues::Float32x2(v) => Some(v.clone()),
            _ => None,
        })
        .expect("mesh must have UV_B attribute for VAT");

    let mut uv_buffer = ShaderStorageBuffer::default();
    uv_buffer.set_data(uv_b_data);
    let vertex_uvs = buffers.add(uv_buffer);

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
            range: (Vec3::from(remap_info.os_remap.max) - Vec3::from(remap_info.os_remap.min)),
            inv_y_resolution: 1.0 / y_resolution,
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
            range: (Vec3::from(remap_info.os_remap.max) - Vec3::from(remap_info.os_remap.min)),
            inv_y_resolution: 1.0 / y_resolution,
            instance: buffer.clone(),
        },
    });

    commands.insert_resource(VatEnemyState {
        material,
        flash_material,
        meshes: vec![mesh_lod0, mesh_lod1],
        vertex_uvs,
        vertex_count,
    });
}

/// Generates a simplified LOD mesh via edge-collapse simplification (pure Rust).
/// Keeps the full vertex buffer (preserving VAT UV mappings) but reduces
/// index count to ~25% by collapsing shortest edges first.
fn generate_lod_mesh(source_handle: &Handle<Mesh>, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    let source = meshes
        .get(source_handle)
        .expect("LOD source mesh must exist");

    let positions: &[[f32; 3]] = source
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|attr| match attr {
            bevy::mesh::VertexAttributeValues::Float32x3(v) => Some(v.as_slice()),
            _ => None,
        })
        .expect("mesh must have Float32x3 positions");

    let indices: Vec<u32> = match source.indices().expect("mesh must have indices") {
        Indices::U16(idx) => idx.iter().map(|&i| i as u32).collect(),
        Indices::U32(idx) => idx.clone(),
    };

    let target_count = indices.len() / 4; // 25% of original
    let simplified = simplify_indices(positions, &indices, target_count);

    info!(
        "LOD1 mesh: {} → {} indices ({:.0}% reduction, {} verts preserved)",
        indices.len(),
        simplified.len(),
        (1.0 - simplified.len() as f32 / indices.len() as f32) * 100.0,
        positions.len(),
    );

    let mut lod_mesh = source.clone();
    lod_mesh.insert_indices(Indices::U32(simplified));
    meshes.add(lod_mesh)
}

/// Spawns mesh children (one per LOD tier) with `VisibilityRange`.
/// All share the same VAT material and animation controller.
pub(crate) fn spawn_vat_mesh_child(
    commands: &mut Commands,
    enemy_entity: Entity,
    vat_state: &VatEnemyState,
    models: &Models,
) {
    let base_transform = Transform::from_translation(MESH_OFFSET)
        .with_scale(Vec3::splat(MESH_SCALE))
        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI));

    let controller = VatAnimationController {
        remap_info: models.enemy_remap_info.clone(),
        current_clip: 0, // Will be set by animate_enemies when behavior changes
        speed: 1.0,
        is_playing: true,
        start_time: 0.0,
        offset: 0.0,
    };

    let mut mesh_entities = Vec::with_capacity(vat_state.meshes.len());
    for (i, mesh_handle) in vat_state.meshes.iter().enumerate() {
        let entity = commands
            .spawn((
                base_transform,
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(vat_state.material.clone()),
                controller.clone(),
                LOD_VISIBILITY_RANGES[i].clone(),
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();
        mesh_entities.push(entity);
    }

    commands
        .entity(enemy_entity)
        .add_children(&mesh_entities)
        .insert(VatMeshLink(mesh_entities));
}

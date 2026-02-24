use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::GltfMesh;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::Indices;
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
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

/// Maximum number of unique active animation frames the compute shader can handle.
/// 64 frames × 8323 vertices × 32 bytes = ~17MB GPU buffer.
const MAX_ACTIVE_FRAMES: u32 = 64;

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
}

/// Links an enemy entity to the child mesh entities (one per LOD) that hold
/// `VatAnimationController`, so `animate_enemies` can update the clip.
#[derive(Component)]
pub(crate) struct VatMeshLink(pub Vec<Entity>);

/// Initializes shared VAT rendering resources for enemies: materials, meshes,
/// LOD generation, and compute pre-skinning buffers. Run once when all assets are loaded.
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
    let tex_height = image.texture_descriptor.size.height;

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

    // Create GPU buffers for compute pre-skinning pipeline
    // Pre-skinned buffer: holds position+normal per (frame_slot × vertex)
    // 32 bytes per entry (vec4 position + vec4 normal)
    let pre_skinned_size = MAX_ACTIVE_FRAMES as usize * vertex_count as usize;
    let pre_skinned_zeros: Vec<[f32; 8]> = vec![[0.0; 8]; pre_skinned_size];
    let mut pre_skinned_buf = ShaderStorageBuffer::default();
    pre_skinned_buf.set_data(pre_skinned_zeros);
    let pre_skinned_buffer = buffers.add(pre_skinned_buf);

    // Frame table buffer: one u32 per unique active frame (max MAX_ACTIVE_FRAMES)
    let mut frame_table_buf = ShaderStorageBuffer::default();
    frame_table_buf.set_data(vec![0u32]);
    let frame_table_buffer = buffers.add(frame_table_buf);

    // Instance lookup buffer: one u32 per entity (mesh_tag → frame slot)
    let mut instance_lookup_buf = ShaderStorageBuffer::default();
    instance_lookup_buf.set_data(vec![0u32]);
    let instance_lookup_buffer = buffers.add(instance_lookup_buf);

    let min_pos: Vec3 = remap_info.os_remap.min.into();
    let max_pos: Vec3 = remap_info.os_remap.max.into();
    let range = max_pos - min_pos;

    let material = vat_materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.816, 0.125, 0.125),
            opaque_render_method: bevy::pbr::OpaqueRendererMethod::Forward,
            ..default()
        },
        extension: OpenVatExtension {
            pre_skinned: pre_skinned_buffer.clone(),
            instance_lookup: instance_lookup_buffer.clone(),
            vertex_count: UVec4::new(vertex_count, 0, 0, 0),
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
            pre_skinned: pre_skinned_buffer.clone(),
            instance_lookup: instance_lookup_buffer.clone(),
            vertex_count: UVec4::new(vertex_count, 0, 0, 0),
        },
    });

    commands.insert_resource(VatEnemyState {
        material,
        flash_material,
        meshes: vec![mesh_lod0, mesh_lod1],
    });

    commands.insert_resource(VatComputeResources {
        vat_texture: models.enemy_vat_texture.clone(),
        vertex_uvs,
        frame_table_buffer,
        instance_lookup_buffer,
        pre_skinned_buffer,
        vertex_count,
        min_pos,
        max_pos,
        range,
        tex_height,
        frame_count: remap_info.os_remap.frames,
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

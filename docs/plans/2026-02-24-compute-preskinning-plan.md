# Compute Pre-Skinning Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace per-instance VAT texture fetches in the vertex shader with a compute pre-pass that processes each unique animation frame once, dropping vertex+prepass cost from 37.6ms to ~10-18ms at 10K enemies.

**Architecture:** A compute shader reads the VAT texture once per unique animation frame (~30), writes pre-skinned position+normal into a storage buffer. The vertex shader reads from this buffer instead. A CPU system deduplicates frames and builds lookup tables. Built inside bevy_open_vat, replaces the current path entirely.

**Tech Stack:** Bevy 0.18 render graph, WGSL compute shaders, wgpu storage buffers, WebGPU (native + WASM)

**Design doc:** `docs/plans/2026-02-24-compute-preskinning-design.md`

---

### Task 1: Change current_clip from String to u8

This is independent prep work. The `VatAnimationController.current_clip` field is a `String` that only ever holds one of a few known clip names. Changing it to a `u8` clip ID eliminates per-change heap allocations and enables array indexing instead of HashMap lookups.

**Files:**
- Modify: `crates/bevy_open_vat/src/data.rs` (lines 7-17)
- Modify: `crates/bevy_open_vat/src/asset.rs` (lines 49-56)
- Modify: `crates/bevy_open_vat/src/system.rs` (line 65)
- Modify: `client/src/combat/enemy.rs` (lines 149-161)
- Modify: `client/src/rendering/enemy_vat.rs` (line 200)

**Step 1: Add clip ID constants to data.rs**

In `crates/bevy_open_vat/src/data.rs`, add above the VatAnimationController struct:

```rust
/// Clip IDs are indices into `RemapInfo.clips` (the ordered Vec of clips).
/// Users define clip names at asset load time; the ID is the position in that Vec.
pub type ClipId = u8;
```

Change `VatAnimationController.current_clip` from `String` to `ClipId`:

```rust
pub struct VatAnimationController {
    pub remap_info: Handle<RemapInfo>,
    pub current_clip: ClipId,
    pub start_time: f32,
    pub offset: f32,
    pub speed: f32,
    pub is_playing: bool,
}
```

Update `Default` impl: `current_clip: 0,`

**Step 2: Change RemapInfo to store clips as a Vec + name lookup**

In `crates/bevy_open_vat/src/asset.rs`, change `RemapInfo`:

```rust
#[derive(Debug, Clone, Asset, Deserialize, TypePath)]
pub struct RemapInfo {
    #[serde(rename = "os-remap")]
    pub os_remap: OsRemap,
    /// Animations deserialized from JSON as a name->clip map.
    pub animations: HashMap<String, VatAnimationClip>,
}

impl RemapInfo {
    /// Look up a clip ID by name. Returns None if the name isn't found.
    /// The ID is the sorted index — sorted to ensure deterministic IDs
    /// regardless of JSON key order.
    pub fn clip_id(&self, name: &str) -> Option<ClipId> {
        let mut names: Vec<&String> = self.animations.keys().collect();
        names.sort();
        names.iter().position(|n| n.as_str() == name).map(|i| i as ClipId)
    }

    /// Get clip data by ID.
    pub fn clip(&self, id: ClipId) -> Option<&VatAnimationClip> {
        let mut names: Vec<&String> = self.animations.keys().collect();
        names.sort();
        names.get(id as usize).and_then(|n| self.animations.get(*n))
    }
}
```

Note: We keep `HashMap<String, VatAnimationClip>` for JSON deserialization but add methods to look up by ID. The sorted key order ensures deterministic clip IDs.

**Step 3: Update system.rs to use clip ID**

In `crates/bevy_open_vat/src/system.rs`, line 65, change:

```rust
// Before:
let Some(clip) = remap_info.animations.get(&controller.current_clip) else {
// After:
let Some(clip) = remap_info.clip(controller.current_clip) else {
```

**Step 4: Update client code that sets current_clip**

In `client/src/rendering/enemy_vat.rs`, the controller initialization (line 200) needs the clip ID. Since we don't have the RemapInfo loaded at spawn time (it's an asset handle), use 0 as default (first clip alphabetically = idle):

```rust
current_clip: 0, // Will be set by animate_enemies when behavior changes
```

In `client/src/combat/enemy.rs`, `animate_enemies` (lines 149-161) needs to resolve clip names to IDs. The system already has access to `VatAnimationController.remap_info`, so add a `Res<Assets<RemapInfo>>` parameter:

```rust
fn animate_enemies(
    enemies: Query<(&EnemyBehavior, &VatMeshLink), Changed<EnemyBehavior>>,
    mut controllers: Query<&mut VatAnimationController>,
    remap_infos: Res<Assets<RemapInfo>>,
    time: Res<Time>,
) {
    for (behavior, vat_link) in &enemies {
        let clip_name = match behavior {
            EnemyBehavior::Idle => "Zombie_Idle_Loop",
            EnemyBehavior::Chase => "Zombie_Walk_Fwd_Loop",
            EnemyBehavior::Attack => "Zombie_Scratch",
        };

        let now = time.elapsed_secs();
        for &mesh_entity in &vat_link.0 {
            if let Ok(mut controller) = controllers.get_mut(mesh_entity) {
                let Some(remap_info) = remap_infos.get(&controller.remap_info) else {
                    continue;
                };
                let Some(clip_id) = remap_info.clip_id(clip_name) else {
                    continue;
                };
                if controller.current_clip != clip_id {
                    controller.current_clip = clip_id;
                    controller.start_time = now;
                }
            }
        }
    }
}
```

**Step 5: Verify**

Run: `cargo check --workspace`
Run: `cargo check --workspace --target wasm32-unknown-unknown -p game-client --no-default-features --features web`

Expected: compiles cleanly.

**Step 6: Smoke test**

Run: `just`

Spawn enemies, verify they animate correctly (idle/chase/attack transitions still work).

**Step 7: Commit**

```
git add crates/bevy_open_vat/src/data.rs crates/bevy_open_vat/src/asset.rs crates/bevy_open_vat/src/system.rs client/src/combat/enemy.rs client/src/rendering/enemy_vat.rs
git commit -m "Change VatAnimationController.current_clip from String to u8 clip ID"
```

---

### Task 2: Extract vertex UVs from mesh and store as resource

The compute shader needs to know each vertex's VAT UV (the `uv_b` attribute) to sample the correct texture column. Extract this from the mesh once at initialization and store it for GPU upload.

**Files:**
- Modify: `client/src/rendering/enemy_vat.rs` (VatEnemyState struct + initialize function)

**Step 1: Add vertex UV data to VatEnemyState**

In `client/src/rendering/enemy_vat.rs`, add to the `VatEnemyState` struct:

```rust
#[derive(Resource)]
pub(crate) struct VatEnemyState {
    pub(crate) material: Handle<VatMaterial>,
    pub flash_material: Handle<VatMaterial>,
    meshes: Vec<Handle<Mesh>>,
    /// UV_B attribute extracted from LOD0 mesh — maps vertex_id to VAT texture column.
    /// Uploaded once to GPU for the compute shader.
    pub(crate) vertex_uvs: Handle<ShaderStorageBuffer>,
    /// Number of vertices in the mesh (needed by compute dispatch and vertex shader).
    pub(crate) vertex_count: u32,
}
```

**Step 2: Extract UV_B in initialize_vat_enemy_resources**

In the `initialize_vat_enemy_resources` function, after getting the LOD0 mesh handle (line 87), extract the UV_B attribute:

```rust
let mesh_lod0 = gltf_mesh.primitives[0].mesh.clone();

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
```

Add `vertex_uvs` and `vertex_count` to the `VatEnemyState` resource insert at the bottom of the function.

**Step 3: Verify**

Run: `cargo check --workspace`

Expected: compiles. The UV data is extracted and stored but not yet used by any shader.

**Step 4: Commit**

```
git add client/src/rendering/enemy_vat.rs
git commit -m "Extract vertex UV_B from mesh for compute pre-skinning"
```

---

### Task 3: Write the compute shader

Create the WGSL compute shader that reads the VAT texture for each unique animation frame and writes pre-skinned positions+normals to a storage buffer.

**Files:**
- Create: `crates/bevy_open_vat/assets/shaders/openvat_compute.wgsl`

**Step 1: Write the compute shader**

```wgsl
// Compute pre-skinning shader for VAT.
// Reads the VAT texture once per unique animation frame, writes
// pre-skinned position+normal to a storage buffer for vertex shaders.

struct VatParams {
    min_pos: vec3<f32>,
    frame_count: u32,
    max_pos: vec3<f32>,
    tex_height: u32,
    range: vec3<f32>,
    vertex_count: u32,
};

struct PreSkinnedVertex {
    position: vec4<f32>,
    normal: vec4<f32>,
};

@group(0) @binding(0) var vat_texture: texture_2d<f32>;
@group(0) @binding(1) var<storage, read> vertex_uvs: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read> frame_table: array<u32>;
@group(0) @binding(3) var<uniform> params: VatParams;
@group(0) @binding(4) var<storage, read_write> pre_skinned: array<PreSkinnedVertex>;

@compute @workgroup_size(1, 64, 1)
fn preskin(@builtin(global_invocation_id) id: vec3<u32>) {
    let frame_slot = id.x;
    let vertex_id = id.y;

    if vertex_id >= params.vertex_count {
        return;
    }
    if frame_slot >= arrayLength(&frame_table) {
        return;
    }

    let abs_frame = frame_table[frame_slot];
    let uv_x = vertex_uvs[vertex_id].x;

    // Integer texel coordinates for textureLoad (no sampler needed)
    let tex_width = textureDimensions(vat_texture).x;
    let texel_x = u32(round(uv_x * f32(tex_width - 1u)));
    let texel_y = abs_frame;

    // Position: top half of texture
    let pos_raw = textureLoad(vat_texture, vec2<u32>(texel_x, texel_y), 0).rgb;

    // Normal: bottom half of texture (offset by tex_height / 2)
    let norm_raw = textureLoad(vat_texture, vec2<u32>(texel_x, texel_y + params.tex_height / 2u), 0).rgb;

    // Blender (Z-up RH) -> Bevy (Y-up RH) coordinate conversion + decoding
    let position = vec4<f32>(
        params.min_pos.x + pos_raw.x * params.range.x,
        params.min_pos.z + pos_raw.z * params.range.z,
        -(params.min_pos.y + pos_raw.y * params.range.y),
        0.0
    );

    var n = norm_raw * 2.0 - 1.0;
    let normal = vec4<f32>(n.x, n.z, -n.y, 0.0);

    let idx = frame_slot * params.vertex_count + vertex_id;
    pre_skinned[idx] = PreSkinnedVertex(position, normal);
}
```

**Step 2: Commit**

```
git add crates/bevy_open_vat/assets/shaders/openvat_compute.wgsl
git commit -m "Add compute pre-skinning shader for VAT"
```

---

### Task 4: Create the render graph compute node

This is the most complex task. Create a Bevy render graph node that:
1. Extracts frame table + instance lookup data from the main world
2. Creates/manages GPU buffers and the compute pipeline
3. Dispatches the compute shader before the main render pass

**Files:**
- Create: `crates/bevy_open_vat/src/compute.rs`
- Modify: `crates/bevy_open_vat/src/lib.rs` (add module)
- Modify: `crates/bevy_open_vat/src/plugin.rs` (register node + shader handle)
- Modify: `crates/bevy_open_vat/Cargo.toml` (may need additional bevy features)

**Step 1: Add the compute module**

In `crates/bevy_open_vat/src/lib.rs`, add:
```rust
pub(crate) mod compute;
```

**Step 2: Add compute shader handle in plugin.rs**

In `crates/bevy_open_vat/src/plugin.rs`, add a new handle constant:

```rust
pub const OPENVAT_COMPUTE_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("a1b2c3d4-e5f6-7890-abcd-ef1234567890");
```

In the `build` method, load the compute shader:

```rust
load_internal_asset!(
    app,
    OPENVAT_COMPUTE_SHADER_HANDLE,
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/shaders/openvat_compute.wgsl"
    ),
    Shader::from_wgsl
);
```

**Step 3: Write compute.rs**

This file contains:
- `VatComputeData` — extracted resource holding frame table, instance lookup, and GPU handles
- `ExtractVatCompute` — system to extract data from main world to render world
- `VatComputeNode` — render graph node that dispatches the compute shader
- `VatComputePipeline` — cached compute pipeline resource

This is the most implementation-intensive file. The structure follows Bevy's render graph pattern (similar to the Game of Life compute shader example). Key reference: Bevy's `bevy_render::render_graph::Node` trait.

The exact implementation depends on Bevy 0.18's render graph API, which may have changed from earlier versions. During implementation:

1. Check `bevy::render::render_graph::{Node, RenderGraphContext, NodeRunError}` for the current API
2. Check `bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue}` for GPU access
3. Check how `bevy::render::Extract` works for moving data from main world to render world
4. Reference: `bevy/examples/shader/compute_shader_game_of_life.rs` for the pattern

The node must be ordered to run BEFORE `bevy::core_pipeline::core_3d::graph::Node3d::MainOpaquePass` and before the prepass.

**Step 4: Register in plugin.rs**

In the `build` method, after adding systems:
- Add the render graph node
- Add extraction systems
- Initialize the compute pipeline resource

**Step 5: Verify**

Run: `cargo check --workspace`

At this point the compute shader dispatches but the vertex shaders still read from the old VAT texture — so visuals are unchanged. The compute output buffer exists but nothing reads from it yet.

**Step 6: Commit**

```
git add crates/bevy_open_vat/src/compute.rs crates/bevy_open_vat/src/lib.rs crates/bevy_open_vat/src/plugin.rs
git commit -m "Add render graph compute node for VAT pre-skinning"
```

---

### Task 5: Replace update_instance_data with prepare_vat_compute

Replace the CPU system that builds per-entity animation params with one that:
1. Computes snapped frame indices per entity
2. Deduplicates into a frame table
3. Builds instance lookup (entity → frame slot)
4. Uploads both to the GPU for the compute node to consume

**Files:**
- Modify: `crates/bevy_open_vat/src/system.rs` (full rewrite)
- Modify: `crates/bevy_open_vat/src/data.rs` (remove VatInstanceData, add VatComputeInput resource)

**Step 1: Add VatComputeInput resource to data.rs**

```rust
/// CPU-side data prepared each frame for the compute pre-skinning pipeline.
/// Extracted to the render world by the compute node.
#[derive(Resource, Default)]
pub struct VatComputeInput {
    /// Unique absolute frame indices currently in use, one per slot.
    pub frame_table: Vec<u32>,
    /// Per-entity mapping: instance_lookup[mesh_tag] = frame slot index.
    pub instance_lookup: Vec<u32>,
}
```

Export in prelude.

**Step 2: Rewrite system.rs**

Replace `update_instance_data` with `prepare_vat_compute`:

```rust
use bevy::{mesh::MeshTag, pbr::ExtendedMaterial, prelude::*};

use crate::{
    asset::RemapInfo,
    data::{VatAnimationController, VatComputeInput},
    material::OpenVatExtension,
};

/// Prepares frame table and instance lookup for the compute pre-skinning pass.
/// Replaces the old `update_instance_data` system.
#[allow(clippy::too_many_arguments)]
pub fn prepare_vat_compute(
    mut commands: Commands,
    changed_query: Query<Entity, Changed<VatAnimationController>>,
    controller_query: Query<(Entity, &VatAnimationController, Option<&MeshTag>)>,
    remap_infos: Res<Assets<RemapInfo>>,
    mut remap_events: MessageReader<AssetEvent<RemapInfo>>,
    time: Res<Time>,
    mut input: ResMut<VatComputeInput>,
    mut last_count: Local<usize>,
    mut frame_map: Local<HashMap<u32, u32>>,
) {
    let current_count = controller_query.iter().len();
    let any_changed = !changed_query.is_empty();
    let asset_changed = !remap_events.is_empty();
    remap_events.clear();

    if !any_changed && *last_count == current_count && !asset_changed {
        return;
    }
    *last_count = current_count;

    frame_map.clear();
    input.frame_table.clear();
    input.instance_lookup.clear();
    input.instance_lookup.resize(current_count, 0);

    let now = time.elapsed_secs();

    for (index, (entity, controller, existing_tag)) in controller_query.iter().enumerate() {
        let target_tag = index as u32;
        let needs_tag_update = match existing_tag {
            Some(tag) => tag.0 != target_tag,
            None => true,
        };
        if needs_tag_update {
            commands.entity(entity).insert(MeshTag(target_tag));
        }

        // Compute snapped absolute frame index
        let Some(remap_info) = remap_infos.get(&controller.remap_info) else {
            continue;
        };
        let Some(clip) = remap_info.clip(controller.current_clip) else {
            continue;
        };

        let duration = clip.duration().unwrap_or(1.0);
        let speed = if controller.is_playing { controller.speed } else { 0.0 };
        let rate = speed / duration;
        let offset = -(controller.start_time * rate) + controller.offset;
        let raw_progress = now * rate + offset;
        let progress = raw_progress.fract().abs();
        let clip_frame_count = clip.end_frame - clip.start_frame;
        let relative_frame = (progress * clip_frame_count as f32).round() as u32;
        let absolute_frame = clip.start_frame + relative_frame.min(clip_frame_count.saturating_sub(1));

        // Deduplicate: assign or reuse slot
        let next_slot = frame_map.len() as u32;
        let slot = *frame_map.entry(absolute_frame).or_insert_with(|| {
            input.frame_table.push(absolute_frame);
            next_slot
        });

        input.instance_lookup[index] = slot;
    }
}
```

**Step 3: Update plugin.rs**

Change the system registration from `update_instance_data` to `prepare_vat_compute`:

```rust
.add_systems(PostUpdate, prepare_vat_compute);
```

Also init the resource:

```rust
app.init_resource::<VatComputeInput>();
```

**Step 4: Remove VatInstanceData**

In `data.rs`, remove the `VatInstanceData` struct and its `Default` impl (lines 33-51). It's no longer used. Update any imports that reference it.

**Step 5: Verify**

Run: `cargo check --workspace`

At this point the old instance data buffer is no longer populated. The vertex shaders still reference it, so enemies will render incorrectly (expected — we fix the shaders in the next task).

**Step 6: Commit**

```
git add crates/bevy_open_vat/src/system.rs crates/bevy_open_vat/src/data.rs crates/bevy_open_vat/src/plugin.rs
git commit -m "Replace update_instance_data with prepare_vat_compute system"
```

---

### Task 6: Rewrite vertex shaders + update material bindings

Rewrite both vertex shaders to read from the pre-skinned buffer instead of the VAT texture. Update the material extension to bind the new buffers.

**Files:**
- Modify: `crates/bevy_open_vat/assets/shaders/openvat_pbr.wgsl` (full rewrite)
- Modify: `crates/bevy_open_vat/assets/shaders/openvat_prepass.wgsl` (full rewrite)
- Modify: `crates/bevy_open_vat/src/material.rs` (change bindings)
- Modify: `client/src/rendering/enemy_vat.rs` (update material creation)

**Step 1: Update OpenVatExtension bindings**

In `crates/bevy_open_vat/src/material.rs`, replace the current bindings:

```rust
#[derive(Debug, Default, Clone, Asset, AsBindGroup, Reflect)]
pub struct OpenVatExtension {
    /// Pre-skinned vertex data (position + normal per slot×vertex).
    /// Written by compute shader each frame.
    #[storage(100, visibility(vertex), read_only)]
    pub pre_skinned: Handle<ShaderStorageBuffer>,

    /// Per-entity mapping: instance_lookup[mesh_tag] = frame slot.
    #[storage(101, visibility(vertex), read_only)]
    pub instance_lookup: Handle<ShaderStorageBuffer>,

    /// Number of vertices in the mesh (for buffer indexing).
    #[uniform(102, visibility(vertex))]
    pub vertex_count: u32,

    // Padding to satisfy uniform alignment
    #[uniform(102, visibility(vertex))]
    pub _pad0: u32,
    #[uniform(102, visibility(vertex))]
    pub _pad1: u32,
    #[uniform(102, visibility(vertex))]
    pub _pad2: u32,
}
```

Note: The exact uniform layout needs to be verified against WGSL alignment rules. The vertex_count could also go into the pre-skinned buffer metadata or a separate uniform.

**Step 2: Rewrite openvat_pbr.wgsl**

```wgsl
#import bevy_pbr::mesh_functions;
#import bevy_pbr::mesh_functions::get_world_from_local;
#import bevy_pbr::mesh_functions::mesh_position_local_to_world;
#import bevy_pbr::mesh_functions::mesh_normal_local_to_world;
#import bevy_pbr::view_transformations::position_world_to_clip
#import bevy_pbr::forward_io::VertexOutput;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv_b: vec2<f32>,
}

struct PreSkinnedVertex {
    position: vec4<f32>,
    normal: vec4<f32>,
};

struct VatVertexParams {
    vertex_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<storage, read> pre_skinned: array<PreSkinnedVertex>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> instance_lookup: array<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> params: VatVertexParams;

@vertex
fn main(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let tag = mesh_functions::get_tag(vertex.instance_index);
    let safe_tag = tag % arrayLength(&instance_lookup);
    let slot = instance_lookup[safe_tag];
    let idx = slot * params.vertex_count + vertex.vertex_index;
    let skinned = pre_skinned[idx];

    let new_position = vertex.position + skinned.position.xyz;
    let new_normal = skinned.normal.xyz;

    let world_from_local = get_world_from_local(vertex.instance_index);

    out.world_position = mesh_position_local_to_world(world_from_local, vec4<f32>(new_position, 1.0));
    out.world_normal = mesh_normal_local_to_world(new_normal, vertex.instance_index);
    out.position = position_world_to_clip(out.world_position.xyz);

    out.uv = vertex.uv;

    return out;
}
```

**Step 3: Rewrite openvat_prepass.wgsl**

Same pattern but skip the normal read:

```wgsl
#import bevy_pbr::mesh_functions;
#import bevy_pbr::mesh_functions::get_world_from_local;
#import bevy_pbr::mesh_functions::mesh_position_local_to_world;
#import bevy_pbr::view_transformations::position_world_to_clip
#import bevy_pbr::prepass_io::VertexOutput;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv_b: vec2<f32>,
}

struct PreSkinnedVertex {
    position: vec4<f32>,
    normal: vec4<f32>,
};

struct VatVertexParams {
    vertex_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<storage, read> pre_skinned: array<PreSkinnedVertex>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> instance_lookup: array<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> params: VatVertexParams;

@vertex
fn main(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let tag = mesh_functions::get_tag(vertex.instance_index);
    let safe_tag = tag % arrayLength(&instance_lookup);
    let slot = instance_lookup[safe_tag];
    let idx = slot * params.vertex_count + vertex.vertex_index;
    let skinned = pre_skinned[idx];

    let new_position = vertex.position + skinned.position.xyz;

    let world_from_local = get_world_from_local(vertex.instance_index);

    out.world_position = mesh_position_local_to_world(world_from_local, vec4<f32>(new_position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif

#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif

    return out;
}
```

**Step 4: Update material creation in enemy_vat.rs**

In `client/src/rendering/enemy_vat.rs`, the `initialize_vat_enemy_resources` function creates `OpenVatExtension`. Update it to use the new bindings:

```rust
extension: OpenVatExtension {
    pre_skinned: pre_skinned_buffer.clone(),
    instance_lookup: instance_lookup_buffer.clone(),
    vertex_count,
    _pad0: 0,
    _pad1: 0,
    _pad2: 0,
},
```

The `pre_skinned_buffer` and `instance_lookup_buffer` handles need to be created here (or passed from the compute node setup). The compute node will write to these buffers each frame.

**Step 5: Verify**

Run: `cargo check --workspace`

**Step 6: Visual test**

Run: `just`

Spawn enemies. They should render with correct animated positions and normals — but now powered by the compute pre-skinning pipeline instead of per-instance texture fetches.

If enemies appear in T-pose or at origin: the compute shader isn't writing data correctly, or the buffer indexing is off. Debug by checking frame_table contents and buffer sizes.

**Step 7: Commit**

```
git add crates/bevy_open_vat/assets/shaders/ crates/bevy_open_vat/src/material.rs client/src/rendering/enemy_vat.rs
git commit -m "Rewrite vertex shaders to read from compute pre-skinned buffer"
```

---

### Task 7: GPU profile and tune

Verify the performance improvement with the GPU profiler.

**Files:** None (testing only)

**Step 1: Baseline comparison**

Run: `just`

Spawn 10K enemies, stand in the center, press F10 for GPU profiler.

Compare against baseline:
```
Baseline (before):   46.9ms / 21.3 FPS
Vertex + prepass:    37.6ms
```

Expected improvement: vertex+prepass drops to 10-18ms, total frame time drops to ~20-30ms, FPS reaches 33-50.

**Step 2: Check compute dispatch cost**

The profiler's "Enemies Hidden" phase hides mesh entities but the compute shader still runs (it dispatches based on frame table, not visibility). If "Enemies Hidden" is notably higher than the 8.5ms baseline, the compute dispatch itself has measurable cost. It should be negligible (<0.5ms for 30 frames × 8323 vertices).

**Step 3: Tune workgroup size**

If performance is lower than expected, try different workgroup sizes in the compute shader:
- `@workgroup_size(1, 64, 1)` — current
- `@workgroup_size(1, 128, 1)` — larger workgroups, fewer dispatches
- `@workgroup_size(1, 256, 1)` — maximum for most GPUs

Rerun profiler after each change.

**Step 4: Document results**

Log the before/after GPU profiler output for future reference.

---

### Task 8: Clean up and update flash material

The flash material (white+emissive hit effect) also uses OpenVatExtension and needs to work with the new bindings.

**Files:**
- Modify: `client/src/rendering/enemy_vat.rs` (flash material creation)
- Modify: `client/src/combat/vfx.rs` (verify flash still works)

**Step 1: Update flash material**

The flash material in `initialize_vat_enemy_resources` needs the same `pre_skinned` and `instance_lookup` buffer handles as the main material (they share the same compute output):

```rust
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
        vertex_count,
        _pad0: 0,
        _pad1: 0,
        _pad2: 0,
    },
});
```

**Step 2: Test**

Run: `just`

Spawn enemies, attack them. Verify the white flash effect still appears on hit.

**Step 3: Commit**

```
git add client/src/rendering/enemy_vat.rs
git commit -m "Update flash material for compute pre-skinning pipeline"
```

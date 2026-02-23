#import bevy_pbr::mesh_functions;
#import bevy_pbr::mesh_functions::get_world_from_local;
#import bevy_pbr::mesh_functions::mesh_position_local_to_world;
#import bevy_pbr::view_transformations::position_world_to_clip
#import bevy_pbr::prepass_io::{Vertex, VertexOutput};
#import bevy_render::globals::Globals
@group(0) @binding(1) var<uniform> globals: Globals;

// --- Structures ---

struct OpenVatParams {
    min_pos: vec3<f32>,
    frame_count: u32,
    max_pos: vec3<f32>,
    y_resolution: f32,
    range: vec3<f32>,
    inv_y_resolution: f32,
};

struct VatInstanceData {
    start_frame: u32,
    frame_count: u32,
    rate: f32,
    offset: f32,
};


// --- Bindings ---

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var vat_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var vat_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> ext: OpenVatParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var<storage, read> instance_data: array<VatInstanceData>;

// --- Utility Functions ---

fn get_vat_data_safe(tag: u32) -> VatInstanceData {
    let safe_tag = tag % arrayLength(&instance_data);
    return instance_data[safe_tag];
}

// Prepass only needs position for depth — skip normal texture fetch entirely.
fn apply_vat_position(frame_index: f32, v_pos: vec3<f32>, uv_vat: vec2<f32>) -> vec3<f32> {
    let frame_cnt = f32(ext.frame_count);
    let safe_frame = frame_index % frame_cnt;

    // Snap to nearest frame — avoids a second texture fetch and mix().
    let nearest_frame = round(safe_frame) % frame_cnt;

    let frame_step = ext.inv_y_resolution;
    let uv = uv_vat + vec2<f32>(0.0, nearest_frame * frame_step);

    let pos = textureSampleLevel(vat_texture, vat_sampler, uv, 0).rgb;

    // [Coordinate System Conversion]
    // Blender (Z-up Right-handed) -> Bevy (Y-up Right-handed)
    // Bevy.x = Blender.x, Bevy.y = Blender.z, Bevy.z = -Blender.y
    return v_pos + vec3<f32>(
        ext.min_pos.x + pos.x * ext.range.x,
        ext.min_pos.z + pos.z * ext.range.z,
        -(ext.min_pos.y + pos.y * ext.range.y)
    );
}

// --- Vertex Shader ---

@vertex
fn main(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let tag = mesh_functions::get_tag(vertex.instance_index);
    let my_data = get_vat_data_safe(tag);

    let raw_progress = globals.time * my_data.rate + my_data.offset;
    let progress = fract(raw_progress);

    let relative_frame = progress * f32(my_data.frame_count);
    let absolute_frame = f32(my_data.start_frame) + relative_frame;

    let new_position = apply_vat_position(absolute_frame, vertex.position, vertex.uv_b);

    let world_from_local = get_world_from_local(vertex.instance_index);

    // Local -> World (Position)
    out.world_position = mesh_position_local_to_world(world_from_local, vec4<f32>(new_position, 1.0));

    // World -> Clip
    out.position = position_world_to_clip(out.world_position.xyz);

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif

#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif

    return out;
}

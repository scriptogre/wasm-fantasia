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

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<storage, read> pre_skinned: array<PreSkinnedVertex>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> instance_lookup: array<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> vertex_params: vec4<u32>;

@vertex
fn main(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let tag = mesh_functions::get_tag(vertex.instance_index);
    let safe_tag = tag % arrayLength(&instance_lookup);
    let slot = instance_lookup[safe_tag];
    let idx = slot * vertex_params.x + vertex.vertex_index;
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

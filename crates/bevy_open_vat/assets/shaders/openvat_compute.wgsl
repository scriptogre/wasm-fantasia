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

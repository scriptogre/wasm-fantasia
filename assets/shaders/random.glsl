#import bevy_pbr::forward_io::VertexOutput

@group(2) @binding(0) var<uniform> color: vec4<f32>;
@group(2) @binding(1) var<uniform> time: f32;
@group(2) @binding(2) var<uniform> depletion: f32; // 0..1

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // 4D input for noise: world position + time
    let pos4 = vec4<f32>(mesh.world_position.xyz * 5.0, time);

    // Compute 3D noise vector using snoise34
    let noise = snoise4(pos4);

    // Apply uniform color and depletion
    return vec4<f32>(noise * color.rgb, color.a) ;
}

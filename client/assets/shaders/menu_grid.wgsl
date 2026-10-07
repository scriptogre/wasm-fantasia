#import bevy_ui::ui_vertex_output::UiVertexOutput
@group(1) @binding(0) var<uniform> background: vec4<f32>;
@group(1) @binding(1) var<uniform> glow: vec4<f32>;
@group(1) @binding(2) var<uniform> ink: vec4<f32>;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv * 2.0 - 1.0;
    let curved = p * (1.0 + 0.15 * p.yx * p.yx);
    let grid = curved * in.size / 144.0;
    let distance = abs(fract(grid - 0.5) - 0.5) / max(fwidth(grid), vec2(0.0001));
    let line = 1.0 - min(min(distance.x, distance.y), 1.0);
    let light = max(0.0, 1.0 - length((in.uv - vec2(0.4, 0.35)) * 1.25));
    let base = mix(background.rgb, glow.rgb, light);
    return vec4(mix(base, ink.rgb, line * 0.007), 1.0);
}

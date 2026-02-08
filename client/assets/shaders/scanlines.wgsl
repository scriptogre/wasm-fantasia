#import bevy_ui::ui_vertex_output::UiVertexOutput

// Packed settings: x=frequency, y=intensity, z=scroll_speed, w=time
@group(1) @binding(0)
var<uniform> settings: vec4<f32>;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let frequency = settings.x;
    let intensity = settings.y;
    let scroll_speed = settings.z;
    let time = settings.w;

    // Sharp scan lines (square wave instead of sine for crisp look)
    let scan_y = uv.y * frequency + time * scroll_speed;
    let scan_wave = fract(scan_y);
    let scan_line = select(1.0, 1.0 - intensity, scan_wave < 0.5);

    // Vignette - subtle edge darkening
    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(uv, center);
    let vignette = smoothstep(0.7, 0.4, dist);

    // Only darken, don't lighten
    let alpha = (1.0 - scan_line) + (1.0 - vignette) * 0.2;

    return vec4<f32>(0.0, 0.0, 0.0, clamp(alpha, 0.0, 0.4));
}

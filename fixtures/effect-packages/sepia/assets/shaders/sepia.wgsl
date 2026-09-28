@group(0) @binding(0) var scene_color: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
// Seconds since preparation, reserved zero, viewport width, viewport height.
@group(0) @binding(2) var<uniform> frame: vec4f;

@fragment
fn fs_main(@builtin(position) pixel: vec4f) -> @location(0) vec4f {
    let uv = pixel.xy / frame.zw;
    let color = textureSample(scene_color, scene_sampler, uv).rgb;
    let luma = dot(color, vec3f(0.2126, 0.7152, 0.0722));
    let warm = luma * vec3f(1.22, 0.92, 0.60);
    let vignette = 1.0 - 0.55 * dot(uv - vec2f(0.5), uv - vec2f(0.5));
    let pulse = 0.94 + 0.02 * sin(frame.x);
    return vec4f(mix(color, warm, 0.8) * vignette * pulse, 1.0);
}

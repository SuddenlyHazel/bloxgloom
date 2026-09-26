@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var bloom: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
// exposure, bloom strength, explicit sRGB encoding for non-sRGB targets, effects enabled
@group(0) @binding(3) var<uniform> settings: vec4<f32>;

struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) id: u32) -> Vertex {
    let uv = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    var out: Vertex;
    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return out;
}
fn bright(uv: vec2<f32>) -> vec3<f32> {
    let color = textureSample(scene, linear_sampler, uv).rgb;
    let peak = max(color.r, max(color.g, color.b));
    // Soft knee above ordinary daylight: preserve material detail without a scene-wide haze.
    let knee = clamp(peak - 0.9, 0.0, 0.6);
    let amount = max(peak - 1.2, knee * knee / 1.2);
    return color * amount / max(peak, 0.00001);
}
@fragment fn extract(input: Vertex) -> @location(0) vec4<f32> {
    let d = 1.0 / vec2<f32>(textureDimensions(scene));
    let color = bright(input.uv + d * vec2<f32>(-1.0, -1.0))
        + bright(input.uv + d * vec2<f32>(1.0, -1.0))
        + bright(input.uv + d * vec2<f32>(-1.0, 1.0))
        + bright(input.uv + d * vec2<f32>(1.0, 1.0));
    return vec4<f32>(color * 0.25, 1.0);
}
fn blur(uv: vec2<f32>, axis: vec2<f32>) -> vec4<f32> {
    let d = axis / vec2<f32>(textureDimensions(scene));
    var color = textureSample(scene, linear_sampler, uv).rgb * 0.227027;
    color += textureSample(scene, linear_sampler, uv + d * 1.384615).rgb * 0.316216;
    color += textureSample(scene, linear_sampler, uv - d * 1.384615).rgb * 0.316216;
    color += textureSample(scene, linear_sampler, uv + d * 3.230769).rgb * 0.070270;
    color += textureSample(scene, linear_sampler, uv - d * 3.230769).rgb * 0.070270;
    return vec4<f32>(color, 1.0);
}
@fragment fn horizontal(input: Vertex) -> @location(0) vec4<f32> { return blur(input.uv, vec2<f32>(1.0, 0.0)); }
@fragment fn vertical(input: Vertex) -> @location(0) vec4<f32> { return blur(input.uv, vec2<f32>(0.0, 1.0)); }

// Neutral shoulder: retain hue through highlight compression; black remains black.
fn tone_map(value: vec3<f32>) -> vec3<f32> {
    let x = min(value.r, min(value.g, value.b));
    let offset = select(0.04, x - 6.25 * x * x, x < 0.08);
    var color = value - offset;
    let peak = max(color.r, max(color.g, color.b));
    if peak < 0.76 { return color; }
    let new_peak = 1.0 - 0.24 * 0.24 / (peak + 0.24 - 0.76);
    color *= new_peak / peak;
    let desaturation = 1.0 - 1.0 / (0.15 * (peak - new_peak) + 1.0);
    return mix(color, vec3<f32>(new_peak), desaturation);
}
@fragment fn composite(input: Vertex) -> @location(0) vec4<f32> {
    var hdr = textureSample(scene, linear_sampler, input.uv).rgb;
    var color = max(hdr, vec3<f32>(0.0));
    if settings.w > 0.0 {
        if settings.y > 0.0 { hdr += textureSample(bloom, linear_sampler, input.uv).rgb * settings.y; }
        color = tone_map(max(hdr * settings.x, vec3<f32>(0.0)));
    }
    if settings.z > 0.0 {
        color = select(1.055 * pow(color, vec3<f32>(1.0 / 2.4)) - 0.055, color * 12.92, color <= vec3<f32>(0.0031308));
    }
    return vec4<f32>(color, 1.0);
}

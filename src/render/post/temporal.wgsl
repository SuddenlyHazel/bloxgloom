// Camera/object TAA. Explicit actor motion includes previous presented skinning.
// Reactive surfaces use this frame. Enhanced sky remains current; explicit
// source-reference sky accumulates directional, sky-classified history.
struct Settings {
    inverse_current: mat4x4f,
    previous: mat4x4f,
    // history weight, history valid, relative depth tolerance, reference sky history
    params: vec4f,
    depth_range: vec4f,
};
@group(0) @binding(0) var current: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var history: texture_2d<f32>;
@group(0) @binding(3) var history_depth: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;
@group(0) @binding(5) var<uniform> settings: Settings;
@group(0) @binding(6) var object_motion: texture_2d<f32>;
@group(0) @binding(7) var indirect_reactive: texture_2d<f32>;
struct Vertex { @builtin(position) position: vec4f };
@vertex fn vs_main(@builtin(vertex_index) id: u32) -> Vertex {
    let uv = vec2f(f32((id << 1u) & 2u), f32(id & 2u));
    var out: Vertex;
    out.position = vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}
struct Output { @location(0) color: vec4f, @location(1) depth: f32 };
fn linear_depth(z: f32) -> f32 { return settings.depth_range.x / max(1.0 - z * (1.0 - settings.depth_range.x / settings.depth_range.y), 0.000001); }
@fragment fn resolve(input: Vertex) -> Output {
    let size = vec2i(textureDimensions(current));
    let pixel = vec2i(input.position.xy);
    let uv = input.position.xy / vec2f(size);
    let color = textureLoad(current, pixel, 0).rgb;
    let z = textureLoad(depth, pixel, 0);
    var out: Output;
    out.color = vec4f(color, 1.0);
    out.depth = linear_depth(z);
    let motion_sample = textureLoad(object_motion, pixel, 0);
    let reactive=motion_sample.a<0.0||textureLoad(indirect_reactive,pixel,0).a<0.0;
    let reference_sky=z>=1.0&&settings.params.w>0.5;
    if reference_sky&&!reactive {out.depth=-1.0;}
    if settings.params.y == 0.0 || reactive { return out; }
    if z>=1.0 {
        if reference_sky {out.color=vec4f(bg_reference_temporal_sky(uv,color,pixel,size),1.0);}
        return out;
    }
    var old_uv: vec2f;
    var expected: f32;
    if motion_sample.a > 0.0 {
        old_uv = uv + motion_sample.rg / vec2f(size);
        expected = motion_sample.b;
    } else {
        let world = settings.inverse_current * vec4f(uv * vec2f(2.0, -2.0) + vec2f(-1.0, 1.0), z, 1.0);
        let previous = settings.previous * world;
        if previous.w <= 0.0 { return out; }
        let ndc = previous.xyz / previous.w;
        if ndc.z < 0.0 || ndc.z >= 1.0 { return out; }
        old_uv = ndc.xy * vec2f(0.5, -0.5) + vec2f(0.5) + settings.depth_range.zw;
        expected = linear_depth(ndc.z);
    }
    let half_texel = vec2f(0.5) / vec2f(size);
    if any(old_uv < half_texel) || any(old_uv > vec2f(1.0) - half_texel) { return out; }
    // Check every tap used by the bilinear color sample. A mixed foreground /
    // background history footprint must not bleed across a disocclusion edge.
    let base = vec2i(floor(old_uv * vec2f(size) - vec2f(0.5)));
    for (var y = 0; y <= 1; y++) {
        for (var x = 0; x <= 1; x++) {
            let saved = textureLoad(history_depth, clamp(base + vec2i(x, y), vec2i(0), size - vec2i(1)), 0).r;
            if abs(saved - expected) > max(0.02, expected * settings.params.z) { return out; }
        }
    }
    var lo = color;
    var hi = color;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let sample_color = textureLoad(current, clamp(pixel + vec2i(x, y), vec2i(0), size - vec2i(1)), 0).rgb;
            lo = min(lo, sample_color);
            hi = max(hi, sample_color);
        }
    }
    let old = textureSampleLevel(history, linear_sampler, old_uv, 0.0).rgb;
    let clipped = clamp(old, lo, hi);
    // Reactive weight bounds trails from moving objects, emission, and lighting
    // changes whose movement is not represented by camera reprojection.
    let difference = length(old - color) / max(0.1, max(length(old), length(color)));
    let motion = length((old_uv - uv) * vec2f(size));
    let weight = settings.params.x * (1.0 - clamp(difference, 0.0, 1.0)) / (1.0 + motion * 0.05);
    out.color = vec4f(mix(color, clipped, weight), 1.0);
    return out;
}

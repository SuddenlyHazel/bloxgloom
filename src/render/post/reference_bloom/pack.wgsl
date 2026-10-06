@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var unused_glow: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var<uniform> unused_settings: vec4f;
struct Vertex { @builtin(position) position: vec4f, @location(0) uv: vec2f };
@vertex fn vs_main(@builtin(vertex_index) id: u32) -> Vertex {
    let uv = vec2f(f32((id << 1u) & 2u), f32(id & 2u));
    return Vertex(vec4f(uv * 2.0 - 1.0, 0.0, 1.0), vec2f(uv.x, 1.0 - uv.y));
}
@fragment fn copy(input: Vertex) -> @location(0) vec4f {
    return textureLoad(scene, vec2i(input.position.xy), 0);
}
@fragment fn downsample(input: Vertex) -> @location(0) vec4f {
    // GPU-generated source mip filtering is runtime-specific. Use a defined
    // bilinear box footprint, including all edge texels at odd resolutions.
    let source_size = vec2f(textureDimensions(scene));
    let destination_size = max(floor(source_size * 0.5), vec2f(1.0));
    let uv = input.position.xy / destination_size;
    let d = 0.5 / source_size;
    return (textureSampleLevel(scene, linear_sampler, uv + vec2f(-d.x, -d.y), 0.0)
        + textureSampleLevel(scene, linear_sampler, uv + vec2f(d.x, -d.y), 0.0)
        + textureSampleLevel(scene, linear_sampler, uv + vec2f(-d.x, d.y), 0.0)
        + textureSampleLevel(scene, linear_sampler, uv + d, 0.0)) * 0.25;
}
fn bg_bayer2(position: vec2f) -> f32 {
    let p = floor(position);
    return fract(p.x * 0.5 + p.y * p.y * 0.75);
}
fn bg_bayer8(position: vec2f) -> f32 {
    return bg_bayer2(position * 0.25) * 0.0625
        + bg_bayer2(position * 0.5) * 0.25 + bg_bayer2(position);
}
@fragment fn pack(input: Vertex) -> @location(0) vec4f {
    let dimensions = vec2f(textureDimensions(scene));
    let view = 1.0 / dimensions;
    let k = dimensions.y * 0.8 / min(720.0, dimensions.y);
    let gl_uv = vec2f(input.uv.x, 1.0 - input.uv.y);
    let bloom_coord = gl_uv * k;
    let weights = array<f32, 6>(0.03, 0.15, 0.32, 0.32, 0.15, 0.03);
    var blurred = vec3f(0.0);
    for (var level = 1u; level <= 7u; level++) {
        let scale = exp2(f32(level));
        let coord = (bloom_coord - bg_bloom_offset(level, view)) * scale;
        let padding = vec2f(0.5) + 2.0 * view * scale;
        if all(abs(coord - vec2f(0.5)) < padding) {
            // Same implicit footprint as GLSL texture2D in the scaled atlas:
            // d(sampleUV)/d(pixel) = k * scale / source dimensions.
            let mip = max(0.0, f32(level) + log2(k));
            for (var i = 0u; i < 6u; i++) {
                for (var j = 0u; j < 6u; j++) {
                    let sample_gl = coord + (vec2f(f32(i), f32(j)) - 2.5) * view * k * scale;
                    blurred += textureSampleLevel(scene, linear_sampler,
                        vec2f(sample_gl.x, 1.0 - sample_gl.y), mip).rgb * weights[i] * weights[j];
                }
            }
        }
    }
    let gl_position = vec2f(input.position.x, dimensions.y - input.position.y);
    let dither = (bg_bayer8(gl_position) - 0.5) / 384.0;
    let encoded = clamp(pow(max(blurred, vec3f(0.0)) / 32.0, vec3f(0.25)) + dither, vec3f(0.0), vec3f(1.0));
    return vec4f(encoded, 1.0);
}

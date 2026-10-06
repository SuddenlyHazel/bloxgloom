fn bg_reference_bloom(color: vec3f, uv: vec2f) -> vec3f {
    let dimensions = vec2f(textureDimensions(bloom));
    let view = 1.0 / dimensions;
    let res_scale = 1.25 * min(720.0, dimensions.y) / dimensions.y;
    // Atlas coordinates retain GLSL's bottom-left convention, while textures
    // and fragment UVs use WebGPU's top-left convention.
    let gl_uv = vec2f(uv.x, 1.0 - uv.y);
    var blurred = vec3f(0.0);
    for (var level = 1u; level <= 7u; level++) {
        let atlas_gl = (gl_uv / exp2(f32(level))
            + bg_bloom_offset(level, view) + vec2f(0.5 * view.x, 0.0)) * res_scale;
        let encoded = textureSampleLevel(bloom, linear_sampler,
            vec2f(atlas_gl.x, 1.0 - atlas_gl.y), 0.0).rgb;
        blurred += bg_bloom_decode(encoded) * bg_bloom_radius_weight(level);
    }
    return mix(color, blurred / 15.55, 0.2);
}

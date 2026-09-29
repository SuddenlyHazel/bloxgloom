fn effect_fragment(uv: vec2f) -> vec4f {
    let source = effect_input(uv, 0u);
    return vec4f(source.rgb * vec3f(1.1, 0.8, 0.6), source.a);
}

fn effect_fragment(uv: vec2f) -> vec4f {
    return mix(effect_input(uv, 1u), effect_input(uv, 0u), effect_parameter(0u).x);
}

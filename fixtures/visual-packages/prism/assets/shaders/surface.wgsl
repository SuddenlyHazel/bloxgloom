fn material_vertex(input: BgVertex) -> BgVertex {
    var result = input;
    result.position.y += 0.04 * sin(material_time() * 1.5 + input.position.x);
    return result;
}
fn material_fragment(input: BgSurface) -> BgSurface {
    var result = input;
    let detail = material_texture(input.uv, 1u);
    result.albedo = vec4f(mix(input.albedo.rgb, detail.rgb, 0.25) * material_parameter(0u).rgb, input.albedo.a);
    result.emission = result.albedo.rgb * 0.12;
    return result;
}

// RGB tangent normals use image-down green (DirectX convention). Derivatives
// reconstruct the actual UV frame, including mirrored cube faces and rotating drops.
fn bg_material_normal(input: VertexOutput) -> vec3f {
    let px = dpdx(input.world_position);
    let py = dpdy(input.world_position);
    let ux = dpdx(input.uv);
    let uy = dpdy(input.uv);
    let determinant = ux.x * uy.y - ux.y * uy.x;
    if (material_map_flags[u32(input.layer)] & 1u) == 0u || abs(determinant) < 0.000000000001 {
        return input.normal;
    }
    let texel = textureSampleGrad(material_normal, material_sampler, input.uv, input.layer, ux, uy);
    return bg_normal_frame(input.normal, px, py, ux, uy, normalize(texel.rgb * 2.0 - 1.0));
}

fn bg_material_specular(input: VertexOutput) -> vec4f {
    let ux = dpdx(input.uv);
    let uy = dpdy(input.uv);
    if (material_map_flags[u32(input.layer)] & 2u) == 0u { return vec4f(0.0); }
    let texel = textureSampleGrad(material_specular, material_sampler, input.uv, input.layer, ux, uy);
    return vec4f(texel.rgb, 1.0);
}

// Sun-only GGX highlights. Sky and sun visibility gate every reflected term,
// so normal/specular maps cannot add light to a sealed cave or erase a sun shadow.
fn bg_material_highlight(input: VertexOutput, surface: BgSurface, specular: vec4f) -> vec3f {
    let eye = camera.eye.xyz - input.world_position;
    let v = eye / max(length(eye), 0.0001);
    return bg_specular_light(surface.normal, v, camera.sun, input.sky_level,
        bg_sun_visibility(input.world_position), surface.albedo.rgb, specular);
}

// Legacy RGB and imported labPBR XY/AO normals use image-down green. Derivatives
// reconstruct the actual UV frame, including mirrored cube faces and rotating drops.
fn bg_material_normal(input: VertexOutput, coordinates: MaterialCoordinates) -> vec3f {
    let px = dpdx(input.world_position);
    let py = dpdy(input.world_position);
    let ux = coordinates.dx;
    let uy = coordinates.dy;
    let determinant = ux.x * uy.y - ux.y * uy.x;
    if (material_map_flags[u32(input.layer)].flags & 1u) == 0u || abs(determinant) < 0.000000000001 {
        return input.normal;
    }
    let texel = textureSampleGrad(material_normal, material_sampler, coordinates.uv, bg_material_layer(input.layer), ux, uy);
    return bg_normal_frame(input.normal, px, py, ux, uy, bg_normal_data(texel, (material_map_flags[u32(input.layer)].flags & 16u) != 0u).xyz);
}

fn bg_material_occlusion(input: VertexOutput, coordinates: MaterialCoordinates) -> f32 {
    let flags = material_map_flags[u32(input.layer)].flags;
    if (flags & 17u) != 17u { return 1.0; }
    return textureSampleGrad(material_normal, material_sampler, coordinates.uv,
        bg_material_layer(input.layer), coordinates.dx, coordinates.dy).b;
}
fn bg_material_specular(input: VertexOutput, coordinates: MaterialCoordinates, albedo: vec3f) -> BgPbr {
    if (material_map_flags[u32(input.layer)].flags & 2u) == 0u { return bg_decode_pbr(vec4f(0.0), albedo, false, false); }
    let texel = textureSampleGrad(material_specular, material_sampler, coordinates.uv, bg_material_layer(input.layer), coordinates.dx, coordinates.dy);
    return bg_decode_pbr(texel, albedo, (material_map_flags[u32(input.layer)].flags & 16u) != 0u, true);
}

// Shared atmosphere drives both direct highlights and a roughness-filtered
// analytic sky reflection. This has no scene geometry/probes: indoor reflection
// is only the existing voxel glow/bounce estimate, never a fabricated room map.
fn bg_material_highlight(input: VertexOutput, surface: BgSurface, specular: BgPbr,
    sun_visibility: f32, local_visibility: f32) -> vec3f {
    let eye = camera.eye.xyz - input.world_position;
    let v = eye / max(length(eye), 0.0001);
    let n = normalize(surface.normal);
    let roughness = specular.roughness;
    let reflected = reflect(-v,n);
    // Blend the reflected ray toward broad hemisphere samples as the lobe widens.
    let sharp = bg_environment_radiance(reflected);
    let broad = (bg_environment_radiance(n) + bg_environment_radiance(vec3f(0.0,1.0,0.0))
        + bg_environment_radiance(vec3f(0.0,-1.0,0.0))) / 3.0;
    let environment = mix(sharp,broad,roughness*roughness);
    // Explicit local transport avoids subtracting interpolated directional light:
    // custom vertex hooks may bend normals across a triangle and its terminator.
    let indirect = bg_pbr_environment(n,v,specular,
        environment,input.sky_level*local_visibility,
        bg_local_material_radiance(bg_shadowed_local_light(input.world_position,n,input.local_radiance,input.local_direction),
            input.indirect_bounce.xyz*local_visibility/1.35,vec3f(0.0),1.0),1.0);
    return bg_pbr_sun(n,v,camera.sun,input.sky_level,
        sun_visibility,specular,bg_sun_radiance()) + indirect;
}

// Explicit material metadata, never inferred from alpha or texture names. This
// replaces (rather than adds to) the direct lobe; shadow visibility gates all of
// it, so thin surfaces cannot emit light or illuminate a sealed cave.
fn bg_foliage_direct(normal: vec3f, transmission_normal: vec3f, sun: vec4f, sky: f32, layer: u32, subsurface: f32) -> vec3f {
    let flags = material_map_flags[layer].flags;
    let wrap = f32((flags >> 8u) & 255u) / 255.0;
    let transmission = max(subsurface, f32((flags >> 16u) & 255u) / 255.0);
    return bg_thin_direct(normal, transmission_normal, sun, sky, wrap, transmission, bg_sun_radiance());
}

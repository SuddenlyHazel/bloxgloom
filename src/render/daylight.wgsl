// Direction is the confidence-weighted upstream transport vector. Opposing
// paths cancel toward isotropic light; solid voxel occlusion is already baked.
// The shared directionality control reserves unresolved voxel scattering for
// every surface (default35%), without introducing exterior light in sealed rooms.
fn bg_local_light(normal: vec3f, radiance: vec3f, direction: vec3f) -> vec3f {
    let confidence = clamp(length(direction), 0.0, 1.0) * camera.ambient_lower.w;
    let lambert = max(dot(normalize(normal), direction / max(length(direction), 0.00001)), 0.0);
    return radiance * mix(1.0, lambert, confidence);
}
// Atmosphere radiance is packed once after weather and lighting controls.
// All consumers use this linear-light basis, with no automatic exposure.
fn bg_sun_radiance() -> vec3f { return camera.sun_radiance.xyz; }
fn bg_environment_radiance(direction: vec3f) -> vec3f {
    // This is an analytic sky, not a ground/scene reflection. Downward rays
    // must not return blue horizon light; local transport is supplied separately.
    let sky = smoothstep(-0.08, 0.0, direction.y);
    return mix(camera.horizon.xyz, camera.sky_zenith.xyz,
        smoothstep(-0.08, 0.86, direction.y)) * camera.sky_zenith.w * sky;
}
fn bg_direct_light(normal: vec3f, sun: vec4f, sky: f32) -> vec3f {
    return sky * max(dot(normal, normalize(sun.xyz)), 0.0) * bg_sun_radiance();
}
fn bg_indirect_daylight(normal: vec3f, sun: vec4f, sky: f32) -> vec3f {
    let hemisphere = clamp(normal.y * 0.5 + 0.5, 0.0, 1.0);
    return sky * mix(camera.ambient_lower.xyz, camera.ambient_upper.xyz, hemisphere);
}
fn bg_surface_light(normal: vec3f, sun: vec4f, sky: f32, local_light: vec3f,
    bounce: vec3f, glow_bounce: vec3f, visibility: f32) -> vec3f {
    // Sky visibility gates every exterior term. Local AO affects indirect only;
    // sealed caves retain their floor, glow and emissive bounce independently.
    return vec3f(0.012, 0.015, 0.022)
        + bg_indirect_daylight(normal, sun, sky) * visibility
        + bg_direct_light(normal, sun, sky)
        + local_light
        + mix(glow_bounce, bounce, sun.w) * (1.35 * visibility);
}
fn bg_contact_light(light: vec3f, normal: vec3f, sun: vec4f, sky: f32, visibility: f32) -> vec3f {
    return max(vec3f(0.0), light - bg_indirect_daylight(normal, sun, sky) * (1.0 - visibility));
}
fn bg_indirect_light(normal: vec3f, sun: vec4f, sky: f32, bounce: vec3f, glow_bounce: vec3f) -> vec3f {
    return bg_indirect_daylight(normal, sun, sky) + mix(glow_bounce, bounce, sun.w) * 1.35;
}

// A common visibility is deliberately conservative when the sky-only contact
// term overlaps different sky/bounce transport. E*visibility reconstructs the
// retained indirect exactly, preventing the AO union from removing direct light.
fn bg_local_indirect_record(sky_fill: vec3f, bounce: vec3f, baked: f32, contact: f32) -> vec4f {
    let visibility=max(baked*contact,0.00001);
    return vec4f((sky_fill*contact+bounce)/visibility,visibility);
}

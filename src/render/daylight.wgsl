// One linear-light basis for terrain and actors. Sky visibility and the day/night
// multiplier gate *all* daylight; the sealed-cave floor is deliberately unchanged.
// Near-neutral side fill preserves warm palette colors without changing exposure.
fn bg_surface_light(normal: vec3f, sun: vec4f, sky: f32, glow: f32,
    bounce: vec3f, glow_bounce: vec3f, visibility: f32) -> vec3f {
    let hemisphere = clamp(normal.y * 0.5 + 0.5, 0.0, 1.0);
    let fill = mix(vec3f(0.30, 0.285, 0.26), vec3f(0.46, 0.49, 0.54), hemisphere);
    let direct = max(dot(normal, normalize(sun.xyz)), 0.0) * vec3f(0.72, 0.67, 0.56);
    // Local occlusion only dims indirect light. No permanent sun/torch shadow is
    // painted onto moving parts, and no ambient energy is added inside caves.
    return vec3f(0.012, 0.015, 0.022)
        + sky * sun.w * (fill * visibility + direct)
        + glow * glow * vec3f(1.0, 0.57, 0.23)
        + mix(glow_bounce, bounce, sun.w) * (1.35 * visibility);
}

fn bg_direct_light(normal: vec3f, sun: vec4f, sky: f32) -> vec3f {
    return sky * sun.w * max(dot(normal, normalize(sun.xyz)), 0.0) * vec3f(0.72, 0.67, 0.56);
}

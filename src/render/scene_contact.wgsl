// Floor-validated actor footprints, with a strict maximum of 32 clipped cells.
// Match geometric top planes only: no projection down ledges or onto actor sides.
fn bg_scene_contact(world: vec3f, normal: vec3f) -> f32 {
    if bg_shadow.contact_params.y <= 0.0 || normal.y < 0.99 { return 1.0; }
    let near = 1.0 - smoothstep(18.0, 28.0, distance(world, bg_shadow.center.xyz));
    if near <= 0.0 { return 1.0; }
    var occlusion = 0.0;
    for (var i = 0u; i < min(u32(bg_shadow.contact_params.x), 32u); i++) {
        let footprint = bg_shadow.contacts[i];
        if abs(world.y - footprint.center.z) > 0.03 || any(world.xz < footprint.bounds.xy) || any(world.xz > footprint.bounds.zw) { continue; }
        let uv = (world.xz - footprint.center.xy) / max(footprint.center.w, 0.001);
        let radial = 1.0 - smoothstep(0.0, 1.0, dot(uv, uv));
        // Overlapping footprints are a union, never a multiplicative stack.
        occlusion = max(occlusion, footprint.light.x * radial * radial);
    }
    return 1.0 - occlusion * near * bg_shadow.contact_params.y;
}

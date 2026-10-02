// A six-metre clear zone keeps nearby blocks and the first-person body crisp.
fn bg_weather_fog(distance: f32, density: f32) -> f32 {
    let optical_depth = max(distance - 6.0, 0.0) * density;
    return 1.0 - exp(-pow(optical_depth, 1.5));
}
fn bg_apply_fog(color: vec3f, world: vec3f, sky: f32) -> vec3f {
    let distance = length(world - camera.eye.xyz);
    let background = smoothstep(camera.fog_range.x, camera.fog_range.y, distance);
    let background_color = mix(vec3f(0.006, 0.009, 0.016), camera.horizon.xyz, sky);
    let clear_color = mix(color, background_color, background);
    // Storm scattering converges to the air's color, regardless of the
    // sky-light value of a shaded face or leaf. Shelter controls air density.
    // Indoors, exterior sky-lit geometry still sees weather through openings.
    // A sealed interior (sheltered viewer and no surface skylight) stays clear.
    let exposure = max(camera.eye.w, smoothstep(0.0, 0.1, sky));
    let density = camera.horizon.w * exposure;
    if density <= 0.0 { return clear_color; }
    let weather = bg_weather_fog(distance, density);
    return mix(clear_color, camera.horizon.xyz, weather);
}

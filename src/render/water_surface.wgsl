// Shared near/distant water response. Normal ripples leave geometry seams flat.
fn bg_water_surface(tint: vec4f, normal: vec3f, sky: f32, glow: f32,
    world: vec3f, relative: vec3f, front: bool, time: f32) -> BgSceneOutput {
    var n = normal;
    if abs(n.y) > 0.5 {
        let ripple = vec2f(sin(world.x*1.4+world.z*0.7+time*0.9),
            cos(world.z*1.7-world.x*0.4-time*0.7))*0.045;
        n = normalize(n+vec3f(ripple.x, 0.0, ripple.y));
    }
    if !front { n = -n; }
    let view = -relative/max(length(relative), 0.00001);
    let fresnel = 0.02+0.98*pow(1.0-max(dot(n, view), 0.0), 5.0);
    let light = bg_surface_light(n, camera.sun, sky, glow*glow*vec3f(1.0, 0.57, 0.23), vec3f(0.0), vec3f(0.0), 1.0);
    let reflection = bg_environment_radiance(reflect(-view, n))*sky;
    let halfway = view+normalize(camera.sun.xyz);
    let half_vector = halfway/max(length(halfway), 0.00001);
    let specular = pow(max(dot(n, half_vector), 0.0), 180.0)*bg_sun_radiance()*sky*0.7;
    let color = bg_apply_fog(mix(tint.rgb*light, reflection, fresnel)+specular, relative+camera.eye.xyz, max(sky, camera.eye.w));
    let alpha = clamp(tint.a+fresnel*(1.0-tint.a), 0.05, 0.96);
    return BgSceneOutput(vec4f(color, alpha), vec4f(0.0, 0.0, 0.0, -1.0));
}

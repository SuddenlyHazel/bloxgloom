struct BgWaterLighting {output:BgSceneOutput,diffuse:vec3f,fresnel:f32};
fn bg_water_lighting(tint: vec4f, normal: vec3f, sky: f32, glow: f32,
    world: vec3f, relative: vec3f, front: bool, time: f32, sun_visibility: f32, footprint:vec4f) -> BgWaterLighting {
    var n = normal;
    var roughness = 0.12;
    if abs(n.y) > 0.5 {
        let waves=bg_water_waves(world,time,footprint);
        n = normalize(n+vec3f(waves.x, 0.0, waves.y));
        roughness = pow(pow(roughness,4.0)+waves.z,0.25);
    }
    if !front { n = -n; }
    let view = -relative/max(length(relative), 0.00001);
    let fresnel = 0.02+0.98*pow(1.0-max(dot(n, view), 0.0), 5.0);
    let light = bg_surface_light(n, camera.sun, sky, glow*glow*vec3f(1.0, 0.57, 0.23), vec3f(0.0), vec3f(0.0), 1.0);
    let reflection_weight = bg_pbr_environment_weight(max(dot(n,view),0.0),roughness,vec3f(0.02));
    let reflection = bg_surface_prefiltered_sky(reflect(-view,n),roughness)*reflection_weight*sky;
    let pbr = BgPbr(roughness,vec3f(0.02),0.0,0.0,0.0,0.0,vec3f(1.0),0u,false,true);
    let visibility=sun_visibility*bg_primary_sun_transmittance(world);
    let specular = bg_pbr_sun(n,view,camera.sun,sky,visibility,pbr,bg_sun_radiance());
    // Direct diffuse and the solar reflection share geometric visibility.
    // Sky/local transport remains independent, including under bridges.
    let shadowed_light = max(light-bg_direct_light(n,camera.sun,sky)*(1.0-visibility),vec3f(0.0));
    let color = tint.rgb*shadowed_light*(1.0-fresnel)+reflection+specular;
    let alpha = clamp(tint.a+fresnel*(1.0-tint.a), 0.05, 0.96);
    let fog_sky = max(sky,camera.eye.w);
    // -2 identifies water independently of reactive foliage (visibility >= -1).
    // Temporal AA still rejects every negative marker; indirect RGB is zero.
    var result = bg_scene_output(color,vec3f(0.0),relative+camera.eye.xyz,fog_sky,-2.0);
    result.color.a = alpha;
    // SSR replaces the same prefiltered fallback already in the color. Its
    // response includes alpha because this receiver blends over opaque ground.
    return BgWaterLighting(bg_scene_reflection(result,n,roughness,length(relative),
        reflection_weight*alpha*bg_fog_transmittance(relative+camera.eye.xyz,fog_sky),sky),
        tint.rgb*shadowed_light*(1.0-fresnel),fresnel);
}
fn bg_water_surface(tint:vec4f,normal:vec3f,sky:f32,glow:f32,world:vec3f,relative:vec3f,
 front:bool,time:f32,sun_visibility:f32,footprint:vec4f)->BgSceneOutput {
 return bg_water_lighting(tint,normal,sky,glow,world,relative,front,time,sun_visibility,footprint).output;
}

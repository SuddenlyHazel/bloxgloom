// Basic surfaces/actors use source default exterior and minimum illumination.
// Near terrain's fragment branch additionally ports raw lightmap, albedo
// balancing, basic SSS, smooth lighting² and default desaturation.
fn bg_surface_light(normal:vec3f,sun:vec4f,sky:f32,local_light:vec3f,
    bounce:vec3f,glow_bounce:vec3f,visibility:f32)->vec3f {
    let scene=bg_bsl_default_scene_light(normal,vec3f(0.0,0.0,1.0),sky,0.0,1.0,bg_bsl_reference_frame());
    let minimum=pow(128.0*0.5/255.0,2.0)*0.04*(1.0-sky*sky);
    return max(scene.rgb+local_light+vec3f(minimum),vec3f(0.0))
        *bg_bsl_vanilla_diffuse(normal)*visibility*visibility;
}

// Preserve the source reference's artistic solar units, including the helper
// used to remove shadowed direct light from water/basic receivers.
fn bg_direct_light(normal:vec3f,sun:vec4f,sky:f32)->vec3f {
    return sky*max(dot(normal,normalize(sun.xyz)),0.0)*bg_sun_radiance();
}

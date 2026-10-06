// Native actor art is matte. Record its actual receiver normal for diffuse GI
// without adding a PBR lobe or changing the BSL reference presentation.
fn bg_actor_scene_output(color:vec3f,indirect:vec3f,world:vec3f,normal:vec3f,sky:f32,history:f32)->BgSceneOutput {
    let output=bg_scene_output(color,indirect,world,sky,history);
    if BG_BSL_REFERENCE {return output;}
    return bg_scene_reflection(output,normalize(normal),1.0,length(camera.eye.xyz-world),vec3f(0.0),sky);
}

// Source entity programs multiply encoded texture and vertex RGB, then pow2.2.
// Engine authored factors/palettes are linear, so recover their encoded values
// independently before that multiplication. No imported pixel changes.
fn bg_actor_reference_encoded(linear:vec3f)->vec3f {
    let positive=max(linear,vec3f(0.0));
    return max(select(1.055*pow(positive,vec3f(1.0/2.4))-0.055,
        positive*12.92,positive<=vec3f(0.0031308)),vec3f(0.0));
}
fn bg_actor_reference_shade(encoded:vec3f,tint:vec3f,normal:vec3f,world:vec3f,
    lightmap:vec2f,shadow:f32)->BgSceneOutput {
    let albedo=pow(max(encoded*bg_actor_reference_encoded(tint),vec3f(0.0)),vec3f(2.2));
    let color=bg_bsl_default_surface(albedo,normal,normalize(camera.eye.xyz-world),
        bg_bsl_reference_lightmap(lightmap,world),1.0,0.0,0.0,shadow,bg_bsl_reference_frame());
    // Source entities use smoothLighting=1 and basicSubsurface=0. Catalog
    // customization/animation remain runtime inputs, not Minecraft entity IDs.
    return bg_scene_output(color,vec3f(0.0),world,lightmap.y,-1.0);
}

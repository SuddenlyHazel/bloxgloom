// Reference water keeps its existing camera aliases and legacy fallback.
// Artistic opaque reflection has its independent source equation in pipeline.
fn bg_surface_prefiltered_sky(reflected:vec3f,roughness:f32)->vec3f {
    var brightness=camera.sun.w;
    var climate=vec2f(0.0);
    if !BG_FOG_REFERENCE {
        brightness=clamp(camera.sun.y/BG_FOG_NOON_HEIGHT,0.0,1.0);
        climate=vec2f(camera.sun_radiance.w,camera.ambient_upper.w);
    }
    return bg_prefiltered_environment(reflected,roughness,camera.horizon.xyz,camera.sky_zenith,
        vec4f(camera.sun.xyz,brightness),climate,BG_FOG_BSL_STYLE&&!BG_FOG_REFERENCE);
}

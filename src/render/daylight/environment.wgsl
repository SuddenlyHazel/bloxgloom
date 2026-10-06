// Raster rough/specular fallback and path misses share the same base HDR sky.
// Clouds and hidden geometry remain scene/proxy responsibilities. Reference
// materials retain their distinct artistic fallback and uniform contract.
fn bg_environment_radiance(direction:vec3f)->vec3f {
    if !BG_FOG_BSL_STYLE {return bg_legacy_environment_radiance(direction);}
    let brightness=clamp(camera.sun.y/BG_FOG_NOON_HEIGHT,0.0,1.0);
    let base=bg_bsl_sky_default(direction,camera.sun.xyz,brightness,
        camera.sun_radiance.w,camera.ambient_upper.w);
    return base*camera.sky_zenith.w*smoothstep(-0.08,0.0,direction.y);
}

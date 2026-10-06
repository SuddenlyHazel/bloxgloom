// A six-metre clear zone keeps nearby blocks and the first-person body crisp.
fn bg_weather_fog(distance: f32, density: f32) -> f32 {
    let optical_depth = max(distance - 6.0, 0.0) * density;
    return 1.0 - exp(-pow(optical_depth, 1.5));
}
fn bg_fog_air_radiance(world:vec3f)->vec3f {
    if !BG_FOG_BSL_STYLE && !BG_FOG_REFERENCE {return camera.horizon.xyz;}
    let relative=world-camera.eye.xyz;let distance=length(relative);
    let direction=relative/max(distance,0.00001);
    var sun=camera.sun.xyz;
    var brightness=clamp(sun.y/BG_FOG_NOON_HEIGHT,0.0,1.0);
    var moon=camera.ambient_upper.w;
    if BG_FOG_REFERENCE {
        // Reference material uniforms carry the active sun/moon light vector.
        sun=select(sun,-sun,camera.ambient_lower.w<0.5 && sun.y>0.0);
        brightness=camera.sun.w;moon=camera.sky_zenith.w;
    }
    let air=bg_bsl_fog_default(direction,distance,sun,brightness,camera.sun_radiance.w,moon);
    if BG_FOG_REFERENCE {
        return bg_bsl_air_fog_exterior(air,camera.eye.w,camera.fog_range.x,camera.fog_range.y);
    }
    return air;
}
fn bg_apply_fog(color: vec3f, world: vec3f, sky: f32) -> vec3f {
    if camera.fog_range.w>0.5 {return color;}
    if BG_FOG_REFERENCE {
        return mix(color,bg_fog_air_radiance(world),bg_reference_fog_amount(world));
    }
    let distance = length(world - camera.eye.xyz);
    let background = smoothstep(camera.fog_range.x, camera.fog_range.y, distance);
    let air=bg_fog_air_radiance(world);
    let background_color = mix(vec3f(0.006, 0.009, 0.016), air, sky);
    let clear_color = mix(color, background_color, background);
    // Storm scattering converges to the air's color, regardless of the
    // sky-light value of a shaded face or leaf. Shelter controls air density.
    // Indoors, exterior sky-lit geometry still sees weather through openings.
    // A sealed interior (sheltered viewer and no surface skylight) stays clear.
    let exposure = max(camera.eye.w, smoothstep(0.0, 0.1, sky));
    let density = camera.horizon.w * exposure;
    if density <= 0.0 { return clear_color; }
    let weather = bg_weather_fog(distance, density);
    return mix(clear_color, air, weather);
}

// Only surface radiance is extinguished. AO must never remove the in-scattered
// horizon/weather color added by bg_apply_fog.
fn bg_fog_transmittance(world: vec3f, sky: f32) -> f32 {
    if camera.fog_range.w>0.5 {return 1.0;}
    if BG_FOG_REFERENCE {return 1.0-bg_reference_fog_amount(world);}
    let distance = length(world-camera.eye.xyz);
    let background = smoothstep(camera.fog_range.x,camera.fog_range.y,distance);
    let exposure = max(camera.eye.w,smoothstep(0.0,0.1,sky));
    let density = camera.horizon.w*exposure;
    var weather = 0.0;
    if density>0.0 { weather=bg_weather_fog(distance,density); }
    return (1.0-background)*(1.0-weather);
}

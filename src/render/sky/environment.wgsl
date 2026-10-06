fn bg_sky_base(direction:vec3f,sun:vec4f,cloud:vec4f,climate:vec2f,horizon:vec3f,zenith:vec3f)->vec3f {
    if cloud.w>0.5 {return bg_bsl_sky_default(direction,sun.xyz,sun.w,climate.x,climate.y);}
    return mix(horizon,zenith,smoothstep(-0.08,0.86,direction.y));
}
// Shared off-screen environment. Sun.w is BSL timeBrightness; cloud packs
// coverage, wind drift X/Z and BSL-style enable. Direct sun is sampled as its
// own light by material/path integrators, avoiding a second solar-disc lobe.
// Climate packs independent rainStrength and server-clock moon multiplier.
fn bg_sky_environment(origin:vec3f,ray:vec3f,sun:vec4f,solar:vec3f,cloud:vec4f,climate:vec2f,horizon:vec3f,zenith:vec3f)->vec3f {
    let base=bg_sky_base(ray,sun,cloud,climate,horizon,zenith);
    let ambient=select(mix(horizon,zenith,0.5)*0.45,bg_bsl_ambient_default(sun.xyz,sun.w,climate.x,climate.y),cloud.w>0.5);
    let volume=bg_cloud_integrate(origin,ray,sun.xyz,solar,ambient,cloud.x,cloud.yz,8u,2u);
    // The sky has no synthetic ground. Geometry/probes provide downward rays.
    return max(volume.rgb+volume.a*base*smoothstep(-0.08,0.0,ray.y),vec3f(0.0));
}

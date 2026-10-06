// The artistic receiver and SSR subtract exactly the same sky radiance.
// Rough scene filtering remains in SSR; this fallback follows source Fresnel
// applied to the view-reflected sky rather than an enhanced GGX convolution.
fn bg_bsl_artistic_environment(ray:vec3f,sun:vec4f,climate:vec2f)->vec3f {
    return bg_bsl_sky_default(ray,sun.xyz,sun.w,climate.x,climate.y);
}

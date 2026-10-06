// Existing spare components carry reference inputs without changing the shared
// camera ABI. Enhanced mode retains its calibrated lighting controls.
fn bg_bsl_reference_frame()->BgBslReferenceFrame {
    return BgBslReferenceFrame(camera.sun_radiance.xyz,camera.ambient_upper.xyz,
        normalize(camera.sun.xyz),camera.ambient_lower.w,camera.sun_radiance.w,
        camera.ambient_upper.w,camera.sky_zenith.w);
}

// Reference-only payload in unused SH slots: relativeEyePosition.xyz, held level.
fn bg_bsl_reference_lightmap(lightmap:vec2f,world:vec3f)->vec2f {
    return bg_bsl_reference_relative_lightmap(lightmap,world-camera.eye.xyz);
}

fn bg_bsl_reference_relative_lightmap(lightmap:vec2f,relative:vec3f)->vec2f {
 return bg_bsl_handlight(lightmap,relative,camera.ambient_sh[5].xyz,camera.ambient_sh[5].w);
}

// Existing spare components carry reference inputs without changing the shared
// camera ABI. Enhanced mode retains its calibrated lighting controls.
fn bg_bsl_reference_frame()->BgBslReferenceFrame {
    return BgBslReferenceFrame(camera.sun_radiance.xyz,camera.ambient_upper.xyz,
        normalize(camera.sun.xyz),camera.ambient_lower.w,camera.sun_radiance.w,
        camera.ambient_upper.w,camera.sky_zenith.w);
}

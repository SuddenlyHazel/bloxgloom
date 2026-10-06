@fragment fn clouds(input:SkyVertex)->@location(0) vec4f {
    let ray=bg_sky_camera_ray(input.uv);
    if BG_REFERENCE_CLOUD_NOISE && abs(sky_camera.reference.w)>0.5 {
        // WGSL fragment Y is top-origin; GLSL gl_FragCoord is bottom-origin.
        let viewport_height=input.position.y/max(1.0-input.uv.y,0.000001);
        let source_pixel=vec2f(input.position.x,input.uv.y*viewport_height);
        return bg_reference_cloud_integrate(sky_camera.eye.xyz,ray,source_pixel,sky_camera);
    }
    let ambient=select(mix(sky_camera.horizon.xyz,sky_camera.zenith.xyz,0.5)*0.45,
        bg_bsl_ambient_default(sky_camera.sun.xyz,sky_camera.sun_radiance.w,sky_camera.climate.x,sky_camera.climate.y),sky_camera.eye.w>0.5);
    return bg_cloud_integrate(sky_camera.eye.xyz,ray,sky_camera.sun.xyz,sky_camera.sun_radiance.xyz,
        ambient,sky_camera.forward.w,vec2f(sky_camera.horizon.w,sky_camera.zenith.w),32u,4u);
}

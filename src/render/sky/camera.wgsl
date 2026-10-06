struct SkyCamera {
    forward:vec4f,right:vec4f,up:vec4f,sun:vec4f,horizon:vec4f,zenith:vec4f,sun_radiance:vec4f,eye:vec4f,climate:vec4f,reference:vec4f,
};
@group(0) @binding(0) var<uniform> sky_camera:SkyCamera;
struct SkyVertex {@builtin(position) position:vec4f,@location(0) uv:vec2f};
@vertex fn vs_main(@builtin(vertex_index) i:u32)->SkyVertex {
    let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));
    return SkyVertex(vec4f(p[i],0.99999,1.0),p[i]*0.5+0.5);
}
fn bg_sky_camera_ray(uv:vec2f)->vec3f {
    let ndc=uv*2.0-1.0;
    return normalize(sky_camera.forward.xyz+sky_camera.right.xyz*ndc.x*sky_camera.right.w+sky_camera.up.xyz*ndc.y*sky_camera.up.w);
}
fn bg_sky_camera_sun()->vec4f {return vec4f(sky_camera.sun.xyz,sky_camera.sun_radiance.w);}
fn bg_sky_camera_cloud()->vec4f {return vec4f(sky_camera.forward.w,sky_camera.horizon.w,sky_camera.zenith.w,sky_camera.eye.w);}

@group(0) @binding(1) var clouds:texture_2d<f32>;
@group(0) @binding(2) var cloud_sampler:sampler;
@group(0) @binding(3) var sun_art:texture_2d<f32>;
@group(0) @binding(4) var moon_art:texture_2d<f32>;
// World-oriented celestial quad projection. Art is sampled from the actual
// JG Java sun and4x2 moon atlas; the source black background adds zero light.
fn bg_sky_celestial_uv(ray:vec3f,direction:vec3f,extent:f32)->vec2f {
    let reference=select(vec3f(0.0,1.0,0.0),vec3f(0.0,0.0,1.0),abs(direction.y)>0.98);
    let right=normalize(cross(direction,reference));let up=cross(right,direction);
    let forward=max(dot(ray,direction),0.0001);
    return vec2f(dot(ray,right),-dot(ray,up))/(2.0*extent*forward)+0.5;
}
fn bg_sky_celestial_inside(uv:vec2f,alignment:f32)->f32 {
    return select(0.0,1.0,alignment>0.0&&all(uv>=vec2f(0.0))&&all(uv<=vec2f(1.0)));
}
struct SkyOutput {@location(0) color:vec4f,@location(1) indirect:vec4f,@location(2) reflection_normal:vec4f,@location(3) reflection_response:vec4f};
@fragment fn fs_main(input:SkyVertex)->SkyOutput {
    let ray=bg_sky_camera_ray(input.uv);
    let sun_direction=normalize(sky_camera.sun.xyz);
    let cloud_parameters=bg_sky_camera_cloud();
    var color=bg_sky_base(ray,bg_sky_camera_sun(),cloud_parameters,sky_camera.climate.xy,sky_camera.horizon.xyz,sky_camera.zenith.xyz);
    let alignment=dot(ray,sun_direction);let solar=sky_camera.sun_radiance.xyz;
    let sun_uv=bg_sky_celestial_uv(ray,sun_direction,0.30);
    let sun_color=textureSampleLevel(sun_art,cloud_sampler,clamp(sun_uv,vec2f(0.0),vec2f(1.0)),0.0).rgb;
    color+=sun_color*solar*bg_sky_celestial_inside(sun_uv,alignment)*6.5;
    let night=1.0-clamp(sun_direction.y*10.0+0.5,0.0,1.0);
    color+=bg_bsl_stars(ray,sky_camera.eye.xyz,sun_direction,sky_camera.climate.w,sky_camera.climate.x,sky_camera.climate.y);
    let moon_uv=bg_sky_celestial_uv(ray,-sun_direction,0.20);
    let phase=u32(sky_camera.climate.z)%8u;
    let tile=vec2f(f32(phase%4u),f32(phase/4u));
    let atlas_uv=(clamp(moon_uv,vec2f(0.0),vec2f(1.0))+tile)/vec2f(4.0,2.0);
    let moon_color=textureSampleLevel(moon_art,cloud_sampler,atlas_uv,0.0).rgb;
    color+=moon_color*bg_sky_celestial_inside(moon_uv,-alignment)*night*vec3f(0.55,0.65,0.85);
    if sky_camera.sun.w<0.5 {
        let volume=textureSampleLevel(clouds,cloud_sampler,vec2f(input.uv.x,1.0-input.uv.y),0.0);
        color=volume.rgb+color*clamp(volume.a,0.0,1.0);
    }
    return SkyOutput(vec4f(max(color,vec3f(0.0)),1.0),vec4f(0.0),vec4f(0.0),vec4f(0.0));
}

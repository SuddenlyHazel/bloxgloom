// Independently expressed supplied default shadows.glsl; host projection depth
// starts in WGPU [0,1] and is converted explicitly to source [-1,1] before warp.
fn bg_bsl_shadow_distortion(raw_xy:vec2f,bias:f32)->f32 {
    return length(raw_xy)*bias+(1.0-bias);
}
fn bg_bsl_distort_shadow(raw:vec3f,bias:f32)->vec3f {
    let factor=bg_bsl_shadow_distortion(raw.xy,bias);
    return vec3f(raw.xy/factor,raw.z*0.2)*0.5+vec3f(0.5);
}
fn bg_bsl_shadow_color(alpha:f32,albedo:vec3f)->vec3f {
    let coverage=1.0-pow(1.0-clamp(alpha,0.0,1.0),1.5);
    return mix(vec3f(1.0),albedo,coverage)*(1.0-pow(clamp(alpha,0.0,1.0),96.0));
}
fn bg_bsl_shadow_result(visibility:f32,averaged_color:vec3f,subsurface:f32)->vec3f {
    let shadow=visibility*mix(visibility,1.0,subsurface);
    return clamp(averaged_color*averaged_color*(1.0-shadow)+vec3f(shadow),vec3f(0.0),vec3f(16.0));
}
fn bg_bsl_shadow_bias(raw:vec3f,world_distance:f32,no_l:f32,subsurface:f32,texel:f32,map_distance:f32,map_bias:f32)->vec2f {
    let factor=bg_bsl_shadow_distortion(raw.xy,map_bias);
    var offset=texel;
    let cosine=max(no_l,0.000001);
    let slope=sqrt(max(0.0,1.0-cosine*cosine))/cosine;
    let scale=factor*map_distance/256.0;
    var bias=(8.0*scale*scale*slope+world_distance*0.005+0.05)*texel;
    if subsurface>0.0 {
        let radial=length(raw.xy);
        let blur=clamp(radial*20.0,0.0,1.0)*(1.0-clamp(radial*10.0-2.0,0.0,1.0))*(1.0-no_l);
        offset=0.0007*(blur*1.5+1.0);bias=0.0002;
    }
    return vec2f(offset,bias);
}

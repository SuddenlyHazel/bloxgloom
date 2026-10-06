// A genuinely three-dimensional cumulus volume, shared by the visible sky,
// scene environment and volumetric/path lighting. Layer bounds follow BSL's
// supplied default height192 + thickness5*scale12. Noise is procedural 3D;
// BSL's texture-driven layered-noise density is a separate artistic model.
const BG_CLOUD_BOTTOM:f32=192.0;
const BG_CLOUD_TOP:f32=252.0;
fn bg_cloud_hash(p:vec3i)->f32 {
    let q=vec3u(p);
    var h=q.x*1597334677u ^ q.y*3812015801u ^ q.z*958689141u ^ 3129154793u;
    h=(h^(h>>16u))*2246822519u;h=(h^(h>>13u))*3266489917u;h=h^(h>>16u);
    return f32(h&16777215u)/16777215.0;
}
fn bg_cloud_noise(p:vec3f)->f32 {
    let cell=vec3i(floor(p));let f=fract(p);let t=f*f*(vec3f(3.0)-2.0*f);
    let low=mix(mix(bg_cloud_hash(cell),bg_cloud_hash(cell+vec3i(1,0,0)),t.x),
        mix(bg_cloud_hash(cell+vec3i(0,1,0)),bg_cloud_hash(cell+vec3i(1,1,0)),t.x),t.y);
    let high=mix(mix(bg_cloud_hash(cell+vec3i(0,0,1)),bg_cloud_hash(cell+vec3i(1,0,1)),t.x),
        mix(bg_cloud_hash(cell+vec3i(0,1,1)),bg_cloud_hash(cell+vec3i(1,1,1)),t.x),t.y);
    return mix(low,high,t.z);
}
fn bg_cloud_density(world:vec3f,coverage:f32,drift:vec2f)->f32 {
    let height=(world.y-BG_CLOUD_BOTTOM)/(BG_CLOUD_TOP-BG_CLOUD_BOTTOM);
    if height<=0.0||height>=1.0 {return 0.0;}
    let profile=smoothstep(0.0,0.12,height)*(1.0-smoothstep(0.55,1.0,height));
    let p=world*vec3f(0.007,0.020,0.007)+vec3f(drift.x,0.0,drift.y)*0.35;
    let base=bg_cloud_noise(p);let detail=bg_cloud_noise(p*2.87+vec3f(31.5,17.8,9.2));
    let cover=clamp(coverage,0.0,1.0);
    let threshold=mix(0.62,0.27,cover);
    let erosion=bg_cloud_noise(p*7.83+vec3f(13.2,7.8,41.5));
    let shape=max(0.0,(base-threshold)*2.8-(1.0-detail)*0.18-(1.0-erosion)*0.09);
    let ceiling=smoothstep(0.75,1.0,cover)*0.17;
    return clamp((shape+ceiling)*profile,0.0,1.0)*0.085;
}
fn bg_cloud_interval(origin:vec3f,direction:vec3f,limit:f32)->vec2f {
    if abs(direction.y)<0.000001 {
        if origin.y>BG_CLOUD_BOTTOM&&origin.y<BG_CLOUD_TOP {return vec2f(0.0,limit);}
        return vec2f(1.0,0.0);
    }
    let lower=(BG_CLOUD_BOTTOM-origin.y)/direction.y;
    let upper=(BG_CLOUD_TOP-origin.y)/direction.y;
    return vec2f(max(min(lower,upper),0.0),min(max(lower,upper),limit));
}
fn bg_cloud_light_transmittance(origin:vec3f,direction:vec3f,coverage:f32,drift:vec2f,samples:u32)->f32 {
    let interval=bg_cloud_interval(origin,direction,6000.0);
    if interval.y<=interval.x {return 1.0;}
    let count=clamp(samples,1u,12u);let step=(interval.y-interval.x)/f32(count);
    var optical_depth=0.0;
    for(var i=0u;i<count;i++) {optical_depth+=bg_cloud_density(origin+direction*(interval.x+(f32(i)+0.5)*step),coverage,drift)*step;}
    return exp(-optical_depth);
}
fn bg_cloud_transmittance(origin:vec3f,direction:vec3f,coverage:f32,drift:vec2f)->f32 {
    return bg_cloud_light_transmittance(origin,direction,coverage,drift,8u);
}
fn bg_cloud_phase(cosine:f32)->f32 {
    let g=0.60;return (1.0-g*g)/pow(max(1.0+g*g-2.0*g*cosine,0.0001),1.5);
}
// RGB is premultiplied in-scattered radiance; alpha is transmittance (not
// opacity). All callers compose radiance + T*background without adding a floor.
fn bg_cloud_integrate(origin:vec3f,direction:vec3f,sun:vec3f,solar:vec3f,ambient:vec3f,
    coverage:f32,drift:vec2f,samples:u32,light_samples:u32)->vec4f {
    let interval=bg_cloud_interval(origin,direction,2400.0);
    if interval.y<=interval.x {return vec4f(0.0,0.0,0.0,1.0);}
    let count=clamp(samples,1u,32u);let step=(interval.y-interval.x)/f32(count);
    var transmittance=1.0;var radiance=vec3f(0.0);
    let phase=bg_cloud_phase(clamp(dot(direction,sun),-1.0,1.0));
    for(var i=0u;i<count;i++) {
        let point=origin+direction*(interval.x+(f32(i)+0.5)*step);
        let extinction=bg_cloud_density(point,coverage,drift);
        if extinction>0.00001 {
            let sample_transmittance=exp(-extinction*step);
            let sun_transmittance=bg_cloud_light_transmittance(point,sun,coverage,drift,light_samples);
            let source=max(ambient,vec3f(0.0))*0.55+max(solar,vec3f(0.0))*sun_transmittance*(0.16+0.11*phase);
            radiance+=transmittance*(1.0-sample_transmittance)*source*0.94;
            transmittance*=sample_transmittance;
        }
        if transmittance<0.008 {break;}
    }
    // Smooth finite-volume visibility avoids a hard cloud curtain where the
    // ray budget ends near the horizon. RGB/T retain premultiplied transport.
    let visibility=1.0-smoothstep(800.0,2200.0,interval.x);
    return vec4f(radiance*visibility,mix(1.0,transmittance,visibility));
}

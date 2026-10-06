// composite5 / post/lensFlare source-default equations. Input UV is GL bottom-up.
struct BgReferenceLens { optical:vec4f, celestial:vec4f, night:vec4f, state:vec4f };
fn bg_lens_base(uv:vec2f,light:vec2f,size:f32,dist:f32,hardness:f32,aspect:f32,fov:f32)->f32 {
    let coord=(uv+(light*dist-0.5))*vec2f(aspect,1.0);
    let lens=clamp(1.0-length(coord)/(size*fov),0.0,1.0/hardness)*hardness;
    return lens*lens*lens*lens;
}
fn bg_lens_overlap(uv:vec2f,l:vec2f,s:f32,a:f32,b:f32,aspect:f32,fov:f32)->f32 {return bg_lens_base(uv,l,s,a,2.0,aspect,fov)*bg_lens_base(uv,l,s,b,2.0,aspect,fov);}
fn bg_lens_point(uv:vec2f,l:vec2f,s:f32,d:f32,aspect:f32,fov:f32)->f32 {return bg_lens_base(uv,l,s,d,1.5,aspect,fov)+bg_lens_base(uv,l,s*4.0,d,1.0,aspect,fov)*0.5;}
fn bg_lens_ring_transform(v:f32)->f32 {return pow(1.0-pow(1.0-pow(v,0.25),10.0),5.0);}
fn bg_lens_ring(uv:vec2f,l:vec2f,s:f32,a:f32,b:f32,aspect:f32,fov:f32)->f32 {
    let v=clamp(bg_lens_ring_transform(bg_lens_base(uv,l,s,b,1.0,aspect,fov))-bg_lens_ring_transform(bg_lens_base(uv,l,s,a,1.0,aspect,fov)),0.0,1.0);
    return v*sqrt(v);
}
fn bg_lens_anamorphic(uv:vec2f,l:vec2f,s:f32,d:f32,aspect:f32,fov:f32)->f32 {
    let coord=abs(uv+(l*d-0.5))*vec2f(aspect*0.07,2.0);
    let lens=clamp(1.0-length(pow(coord/(s*fov),vec2f(0.85)))*4.0,0.0,1.0);return lens*lens*lens;
}
fn bg_lens_rainbow(uv:vec2f,l:vec2f,s:f32,d:f32,rad:f32,aspect:f32,fov:f32)->vec3f {
    let v=clamp(1.0-length((uv+(l*d-0.5))*vec2f(aspect,1.0))/(s*fov),0.0,1.0);
    return vec3f(smoothstep(0.0,rad,v)-smoothstep(rad,rad*2.0,v),smoothstep(rad*0.5,rad*1.5,v)-smoothstep(rad*1.5,rad*2.5,v),smoothstep(rad,rad*2.0,v)-smoothstep(rad*2.0,rad*3.0,v));
}
fn bg_lens_flare(uv:vec2f,aspect:f32,data:BgReferenceLens,factor:f32)->vec3f {
    let l=data.optical.xy;let fov=data.optical.w;
    let base=length(l*vec2f(aspect,1.0));let fade_in=pow(clamp(base*10.0,0.0,1.0),2.0);let fade_out=clamp(base*3.0-1.5,0.0,1.0);
    if fade_out>=0.999 {return vec3f(0.0);}
    var flare=(
        bg_lens_base(uv,l,0.3,-0.45,1.0,aspect,fov)*vec3f(2.2,1.2,0.1)*0.07+
        bg_lens_base(uv,l,0.3,0.10,1.0,aspect,fov)*vec3f(2.2,0.4,0.1)*0.03+
        bg_lens_base(uv,l,0.3,0.30,1.0,aspect,fov)*vec3f(2.2,0.2,0.1)*0.04+
        bg_lens_base(uv,l,0.3,0.50,1.0,aspect,fov)*vec3f(2.2,0.4,2.5)*0.05+
        bg_lens_base(uv,l,0.3,0.70,1.0,aspect,fov)*vec3f(1.8,0.4,2.5)*0.06+
        bg_lens_base(uv,l,0.3,0.95,1.0,aspect,fov)*vec3f(0.1,0.2,2.5)*0.10+
        bg_lens_overlap(uv,l,0.18,-0.30,-0.41,aspect,fov)*vec3f(2.5,1.2,0.1)*0.010+
        bg_lens_overlap(uv,l,0.16,-0.18,-0.29,aspect,fov)*vec3f(2.5,0.5,0.1)*0.020+
        bg_lens_overlap(uv,l,0.15,0.06,0.19,aspect,fov)*vec3f(2.5,0.2,0.1)*0.015+
        bg_lens_overlap(uv,l,0.14,0.15,0.28,aspect,fov)*vec3f(1.8,0.1,1.2)*0.015+
        bg_lens_overlap(uv,l,0.16,0.24,0.37,aspect,fov)*vec3f(1.0,0.1,2.5)*0.015+
        bg_lens_point(uv,l,0.03,-0.55,aspect,fov)*vec3f(2.5,1.6,0.0)*0.20+
        bg_lens_point(uv,l,0.02,-0.40,aspect,fov)*vec3f(2.5,1.0,0.0)*0.15+
        bg_lens_point(uv,l,0.04,0.43,aspect,fov)*vec3f(2.5,0.6,0.6)*0.20+
        bg_lens_point(uv,l,0.02,0.60,aspect,fov)*vec3f(0.2,0.6,2.5)*0.15+
        bg_lens_point(uv,l,0.03,0.67,aspect,fov)*vec3f(0.2,1.6,2.5)*0.25+
        bg_lens_ring(uv,l,0.25,0.43,0.45,aspect,fov)*vec3f(0.10,0.35,2.50)*1.5+
        bg_lens_ring(uv,l,0.18,0.98,0.99,aspect,fov)*vec3f(0.15,1.00,2.55)*2.5
    )*(fade_in-fade_out)+(
        bg_lens_anamorphic(uv,l,1.0,-1.0,aspect,fov)*vec3f(0.3,0.7,1.0)*0.35+
        bg_lens_rainbow(uv,l,0.525,-1.0,0.2,aspect,fov)*0.05+
        bg_lens_rainbow(uv,l,2.0,4.0,0.1,aspect,fov)*0.05
    )*(1.0-fade_out);
    let moon=data.optical.z*0.5+0.5;
    flare=mix(flare,dot(flare,vec3f(0.299,0.587,0.114))*data.night.rgb,moon*0.98);
    return flare*mix(data.celestial.x,data.celestial.y,moon)*factor;
}

// Edge-directed spatial AA in linear HDR, before history and display upscaling.
// This handles current-only wind/water without borrowing stale geometry.
fn bg_temporal_luma(c:vec3f)->f32 {
    let y=max(dot(c,vec3f(0.2126,0.7152,0.0722)),0.0);
    return y/(1.0+y);
}
fn bg_temporal_current(uv:vec2f,pixel:vec2i,size:vec2i)->vec3f {
    let center=textureLoad(current,pixel,0).rgb;
    if settings.spatial.x<0.5 {return center;}
    let texel=1.0/vec2f(size);
    let nw=textureSampleLevel(current,linear_sampler,uv+texel*vec2f(-1.0,-1.0),0.0).rgb;
    let ne=textureSampleLevel(current,linear_sampler,uv+texel*vec2f(1.0,-1.0),0.0).rgb;
    let sw=textureSampleLevel(current,linear_sampler,uv+texel*vec2f(-1.0,1.0),0.0).rgb;
    let se=textureSampleLevel(current,linear_sampler,uv+texel*vec2f(1.0,1.0),0.0).rgb;
    let l=bg_temporal_luma(center);
    let corners=vec4f(bg_temporal_luma(nw),bg_temporal_luma(ne),bg_temporal_luma(sw),bg_temporal_luma(se));
    let lo=min(l,min(min(corners.x,corners.y),min(corners.z,corners.w)));
    let hi=max(l,max(max(corners.x,corners.y),max(corners.z,corners.w)));
    let contrast=hi-lo;
    if contrast<max(0.015,hi*0.125) {return center;}
    var direction=vec2f(-((corners.x+corners.y)-(corners.z+corners.w)),
        (corners.x+corners.z)-(corners.y+corners.w));
    let reduce=max(dot(corners,vec4f(0.25))*0.125,0.0078125);
    direction=clamp(direction/(min(abs(direction.x),abs(direction.y))+reduce),vec2f(-8.0),vec2f(8.0))*texel;
    let a=0.5*(textureSampleLevel(current,linear_sampler,uv-direction/6.0,0.0).rgb
        +textureSampleLevel(current,linear_sampler,uv+direction/6.0,0.0).rgb);
    let b=a*0.5+0.25*(textureSampleLevel(current,linear_sampler,uv-direction*0.5,0.0).rgb
        +textureSampleLevel(current,linear_sampler,uv+direction*0.5,0.0).rgb);
    let lb=bg_temporal_luma(b);
    let edge=select(b,a,lb<lo||lb>hi);
    // Isolated subpixel highlights have no edge direction. A bounded spatial
    // contribution also handles these without temporal trails or HDR clamps.
    let average=(nw+ne+sw+se+center*4.0)/8.0;
    let subpixel=clamp(abs(dot(corners,vec4f(0.25))-l)/max(contrast,0.0001),0.0,1.0);
    return mix(edge,average,subpixel*subpixel*0.25);
}

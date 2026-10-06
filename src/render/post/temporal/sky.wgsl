// Local BSL-reference sky history. Source defaults use YCoCg AABB clipping and
// .7+.2*exp(-pixel motion). The engine sky raster is unjittered and has no
// surface velocity; reproject its direction, not an invented far-plane object.
fn bg_reference_temporal_ycocg(c:vec3f)->vec3f {return vec3f(dot(c,vec3f(0.25,0.5,0.25)),(c.r-c.b)*0.5,dot(c,vec3f(-0.25,0.5,-0.25)));}
fn bg_reference_temporal_rgb(c:vec3f)->vec3f {let n=c.x-c.z;return vec3f(n+c.y,c.x+c.z,n-c.y);}
fn bg_reference_temporal_clip(value:vec3f,lo:vec3f,hi:vec3f)->vec3f {
    let center=(lo+hi)*0.5;let extent=(hi-lo)*0.5+vec3f(0.00000001);
    let delta=value-center;let unit=abs(delta/extent);let longest=max(unit.x,max(unit.y,unit.z));
    return center+delta/max(1.0,longest);
}
fn bg_reference_temporal_sky_uv(uv:vec2f)->vec3f {
    // Terrain inverse_current includes raster jitter, fullscreen sky does not.
    // Add the current jitter before inversion, then omit terrain's post-project
    // jitter offset. This gives zero sky motion for all stationary jitter phases.
    let clip=(uv+settings.depth_range.zw)*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
    let near_world=settings.inverse_current*vec4f(clip,0.0,1.0);
    let far_world=settings.inverse_current*vec4f(clip,1.0,1.0);
    let direction=normalize(far_world.xyz/far_world.w-near_world.xyz/near_world.w);
    let previous=settings.previous*vec4f(direction,0.0);
    if previous.w<=0.0 {return vec3f(-1.0,-1.0,0.0);}
    return vec3f(previous.xy/previous.w*vec2f(0.5,-0.5)+vec2f(0.5),1.0);
}
fn bg_reference_temporal_sky(uv:vec2f,color:vec3f,pixel:vec2i,size:vec2i)->vec3f {
    let reprojected=bg_reference_temporal_sky_uv(uv);
    if reprojected.z<0.5 {return color;}
    let old_uv=reprojected.xy;let half_texel=vec2f(0.5)/vec2f(size);
    if any(old_uv<half_texel)||any(old_uv>vec2f(1.0)-half_texel) {return color;}
    // Reference sky stores a negative depth tag. Every bilinear tap must belong
    // to sky; real surfaces arbitrarily close to far clip remain positive.
    let base=vec2i(floor(old_uv*vec2f(size)-0.5));
    for(var y=0;y<=1;y++) {for(var x=0;x<=1;x++) {
        let p=clamp(base+vec2i(x,y),vec2i(0),size-1);
        if textureLoad(history_depth,p,0).r>=0.0 {return color;}
    }}
    var lo=bg_reference_temporal_ycocg(color);var hi=lo;
    for(var y=-1;y<=1;y++) {for(var x=-1;x<=1;x++) {
        let p=clamp(pixel+vec2i(x,y),vec2i(0),size-1);
        // Foreground/UI/reactive neighbors must not expand the sky history box.
        if textureLoad(depth,p,0)>=1.0&&textureLoad(object_motion,p,0).a>=0.0&&textureLoad(indirect_reactive,p,0).a>=0.0 {
            let c=bg_reference_temporal_ycocg(textureLoad(current,p,0).rgb);lo=min(lo,c);hi=max(hi,c);
        }
    }}
    let old=textureSampleLevel(history,linear_sampler,old_uv,0.0).rgb;
    let clipped=bg_reference_temporal_rgb(bg_reference_temporal_clip(bg_reference_temporal_ycocg(old),lo,hi));
    let motion=length((old_uv-uv)*vec2f(size));
    return mix(color,clipped,0.7+0.2*exp(-motion));
}

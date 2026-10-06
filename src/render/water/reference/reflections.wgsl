// Source SimpleReflection/REFLECTION_MODE=1: current opaque depth and encoded
// deferred reflection image, before ALPHA_BLEND=0 water composition. World
// coordinates preserve view-space lengths; the shared full VP also handles LOD.
fn bg_water_reflection_project(world:vec3f)->vec3f {
    let clip=water_reference.view_projection*vec4f(world,1.0);
    let ndc=clip.xyz/clip.w;
    return vec3f(ndc.x*0.5+0.5,0.5-ndc.y*0.5,ndc.z);
}
fn bg_water_reflection_unproject(uv:vec2f,depth:f32)->vec3f {
    let h=water_reference.inverse_view_projection*vec4f(uv.x*2.0-1.0,1.0-uv.y*2.0,depth,1.0);
    return h.xyz/h.w;
}
@diagnostic(off, derivative_uniformity)
fn bg_water_source_reflection(world:vec3f,normal:vec3f)->vec4f {
    let relative=world-water_reference.eye.xyz;
    let start=world+normal*(length(relative)*0.001+0.025);
    var vector=reflect(normalize(relative),normal);
    var position=world+vector;var traveled=vector;var refinements=0u;
    var projected=vec3f(0.0,0.0,1.0);
    let size=vec2i(textureDimensions(bg_water_opaque_depth));
    for(var i=0u;i<30u;i++) {
        projected=bg_water_reflection_project(position);
        if any(projected.xy<vec2f(-0.05))||any(projected.xy>vec2f(1.05)) {break;}
        let pixel=clamp(vec2i(projected.xy*vec2f(size)),vec2i(0),size-1);
        let sampled=textureLoad(bg_water_opaque_depth,pixel,0);
        let surface=bg_water_reflection_unproject(projected.xy,sampled);
        let error=length(position-surface);
        let tolerance=length(vector)*pow(length(traveled),0.1)*1.3;
        if error<tolerance {
            refinements+=1u;
            if refinements>=4u {break;}
            traveled-=vector;vector*=0.1;
        }
        vector*=2.0;traveled+=vector;position=start+traveled;
    }
    projected.z=min(projected.z,1.0);
    if projected.z>=1.0 {return vec4f(0.0);}
    let border=max(abs(projected.x-0.5),abs(projected.y-0.5))*1.85;
    let fade=clamp(13.333*(1.0-border),0.0,1.0);
    let encoded=textureSample(bg_water_reflection_image,bg_water_reflection_sampler,projected.xy);
    return vec4f(pow(encoded.rgb*2.0,vec3f(8.0)),encoded.a*fade);
}

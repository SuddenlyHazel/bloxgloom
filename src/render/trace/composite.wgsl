// Shared inputs, vertex entry point and world_position are in filter.wgsl.
fn ray_specular_fallback(direction:vec3f,roughness:f32)->vec3f {
    return bg_prefiltered_environment(direction,roughness,ray_frame.horizon.xyz,ray_frame.zenith,
        ray_frame.sun,ray_frame.climate.xy,ray_frame.cloud.w>0.5);
}
struct RayCandidate { radiance:vec4f,geometry:vec4f,position:vec3f,bilinear:f32,sky:bool };
@fragment fn fs_main(@builtin(position) frag:vec4f)->@location(0) vec4f {
    let p=vec2i(frag.xy);var center=textureLoad(receiver,p,0);
    let stride=max(2,i32(ray_frame.parameters.z));
    let media_only=center.z<=0.0||center.w<=0.0;
    let center_sky=media_only&&textureLoad(depth,p,0)>=0.999999;
    let center_water=!media_only&&ray_is_water(textureLoad(indirect,p,0).a);
    let eye_water=ray_frame.water.z>0.5;
    let size=vec2i(textureDimensions(radiance));let full_size=vec2f(textureDimensions(receiver));
    let ndc=(vec2f(p)+0.5)/full_size*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
    let opaque_depth=textureLoad(depth,p,0);
    var opaque_distance=2400.0;
    if opaque_depth<0.999999 {
        let point=ray_frame.inverse*vec4f(ndc,opaque_depth,1.0);
        opaque_distance=length(point.xyz/point.w);
    }
    if media_only {center=vec4f(0.0,0.0,2.0,opaque_distance);}
    var center_position=world_position(vec2f(p),center.w,full_size);
    let center_direction=normalize(center_position-ray_frame.eye.xyz);
    let tolerance=max(0.012,center.w*0.0015);
    let coordinate=(vec2f(p)-f32(stride/2))/f32(stride);
    let base=vec2i(floor(coordinate));let fraction=fract(coordinate);
    var candidates:array<RayCandidate,4>;
    var best=1000000.0;var center_geometry=vec4f(0.0);var center_sample=vec2i(0);
    var nearest_water=1000000.0;var projected_position=center_position;
    for(var i=0u;i<4u;i++) {
        let offset=vec2i(i32(i&1u),i32(i>>1u));let q=clamp(base+offset,vec2i(0),size-vec2i(1));
        var r=textureLoad(radiance,q,0);let g=textureLoad(geometry,q,0);
        r=vec4f(r.rgb+textureLoad(current_correction,q,0).rgb,r.a);
        let sample_pixel=min(q*stride+vec2i(stride/2),vec2i(full_size)-vec2i(1));
        let position=world_position(vec2f(sample_pixel),r.a,full_size);
        let interpolation=select(vec2f(1.0)-fraction,fraction,offset!=vec2i(0));
        var bilinear=interpolation.x*interpolation.y;
        let complete=ray_complete_path(g.z);
        let gn=oct_decode(g.xy);
        var error=abs(dot(center_position-position,gn))+length(center_position-position)*0.002;
        let sample_sky=media_only&&textureLoad(depth,sample_pixel,0)>=0.999999;
        if r.a<=0.0||abs(g.w)<=0.0 {bilinear=0.0;}
        if complete {
            // The raster water receiver may belong to a farther drawn layer.
            // Project the nearest true interface plane to this center ray; do
            // not expose it through an opaque foreground or nonwater center.
            let denominator=dot(center_direction,gn);
            var distance=-1.0;
            if abs(denominator)>0.000001 {distance=dot(position-ray_frame.eye.xyz,gn)/denominator;}
            if (!center_water&&!eye_water)||distance<=0.0||distance>opaque_distance+max(0.10,opaque_distance*0.002) {
                bilinear=0.0;
            }
            if eye_water&&!center_water&&!media_only&&error>tolerance*2.0 {bilinear=0.0;}
            if bilinear>0.0&&distance<nearest_water {
                nearest_water=distance;projected_position=ray_frame.eye.xyz+center_direction*distance;
                center_geometry=g;center_sample=q;best=-1.0;
            }
        } else {
            if (g.z<0.0)!=center_water||(g.z>1.0)!=media_only
                ||abs(ray_surface_roughness(g.z)-center.z)>0.20 {bilinear=0.0;}
            if media_only {
                error=select(abs(center.w-r.a),0.0,center_sky);
                if center_sky!=sample_sky||(!center_sky&&error>max(0.03,center.w*0.01)*2.0) {bilinear=0.0;}
            } else if error>tolerance*2.0 {bilinear=0.0;}
            if bilinear>0.0&&error<best {best=error;center_geometry=g;center_sample=q;}
        }
        candidates[i]=RayCandidate(r,g,position,bilinear,sample_sky);
    }
    if abs(center_geometry.w)<=0.0 {return vec4f(0.0);}
    let complete_path=ray_complete_path(center_geometry.z);
    if complete_path {center_position=projected_position;}
    var n=oct_decode(center_geometry.xy);if media_only&&!complete_path {n=-center_direction;}
    let water=center_geometry.z<0.0;
    let roughness=ray_surface_roughness(center_geometry.z);
    var sum=vec3f(0.0);var weights=0.0;
    for(var i=0u;i<4u;i++) {
        let candidate=candidates[i];let g=candidate.geometry;
        if candidate.bilinear<=0.0||ray_primary_actor(g)!=ray_primary_actor(center_geometry)||(g.w<0.0)!=(center_geometry.w<0.0)
            ||ray_complete_path(g.z)!=complete_path {continue;}
        let gn=oct_decode(g.xy);if dot(n,gn)<0.85 {continue;}
        var weight=0.0;
        if media_only&&!complete_path {
            weight=candidate.bilinear*ray_media_spatial_weight(n,gn,center.w,candidate.radiance.a,0.0,center_sky,candidate.sky);
        } else {
            weight=candidate.bilinear*ray_spatial_weight(center_position,candidate.position,gn,roughness,ray_surface_roughness(g.z),0.0,tolerance);
        }
        sum+=candidate.radiance.rgb*weight;weights+=weight;
    }
    if weights<=0.00001 {return vec4f(0.0);}
    if complete_path {
        // Reconstructed complete-path radiance, including bottom artwork, has
        // been filtered. Cancel only this frame's exact raster baseline here;
        // the baseline itself never enters the temporal/spatial radiance filter.
        return vec4f(sum/weights-textureLoad(baseline,p,0).rgb,0.0);
    }
    let basis=select(ray_filter_basis(textureLoad(indirect,p,0)),vec3f(1.0),water||media_only);
    var correction=sum/weights*basis;
    let primary_t=ray_primary_t(textureLoad(transmission,center_sample,0));
    // Baseline-dependent terms use this frame's exact full-resolution pixels.
    correction-=textureLoad(baseline,p,0).rgb*(1.0-abs(primary_t));
    if !media_only&&primary_t>0.0 {
        let retained=textureLoad(indirect,p,0);
        correction-=retained.rgb*abs(retained.a)*primary_t;
        let v=normalize(ray_frame.eye.xyz-center_position);
        var mapped=oct_decode(center.xy);if dot(mapped,v)<0.0 {mapped=-mapped;}
        let fallback=ray_specular_fallback(reflect(-v,mapped),center.z);
        let specular=textureLoad(response,p,0);
        correction-=fallback*specular.rgb*specular.w*primary_t;
    }
    return vec4f(correction,0.0);
}

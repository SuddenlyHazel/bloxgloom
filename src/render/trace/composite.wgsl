// Shared inputs, vertex entry point and world_position are in filter.wgsl.
struct RayCandidate { radiance:vec4f,geometry:vec4f,position:vec3f,bilinear:f32 };
@fragment fn fs_main(@builtin(position) frag:vec4f)->@location(0) vec4f {
    let p=vec2i(frag.xy);var center=textureLoad(receiver,p,0);
    let stride=max(2,i32(ray_frame.parameters.z));
    let media_only=center.z<=0.0||center.w<=0.0;
    let center_water=!media_only&&ray_is_water(textureLoad(indirect,p,0).a);
    let size=vec2i(textureDimensions(radiance));let full_size=vec2f(textureDimensions(receiver));
    if media_only {
        let ndc=(vec2f(p)+0.5)/full_size*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
        let d=textureLoad(depth,p,0);
        var distance=2400.0;
        if d<0.999999 {
            let point=ray_frame.inverse*vec4f(ndc,d,1.0);
            distance=length(point.xyz/point.w-ray_frame.eye.xyz);
        }
        center=vec4f(0.0,0.0,2.0,distance);
    }
    let center_position=world_position(vec2f(p),center.w,full_size);
    let tolerance=max(0.012,center.w*0.0015);
    // Bracket the actual low-resolution sample centers, including odd strides
    // and the partial last tile; high-frequency shading normals are not planes.
    let coordinate=(vec2f(p)-f32(stride/2))/f32(stride);
    let base=vec2i(floor(coordinate));let fraction=fract(coordinate);
    var candidates:array<RayCandidate,4>;
    var best=1000000.0;var center_geometry=vec4f(0.0);var center_sample=vec2i(0);
    for(var i=0u;i<4u;i++) {
        let offset=vec2i(i32(i&1u),i32(i>>1u));let q=clamp(base+offset,vec2i(0),size-vec2i(1));
        let r=textureLoad(radiance,q,0);let g=textureLoad(geometry,q,0);
        let sample_pixel=min(q*stride+vec2i(stride/2),vec2i(full_size)-vec2i(1));
        let position=world_position(vec2f(sample_pixel),r.a,full_size);
        let interpolation=select(vec2f(1.0)-fraction,fraction,offset!=vec2i(0));
        var bilinear=interpolation.x*interpolation.y;
        if r.a<=0.0||abs(g.w)<=0.0||(g.z<0.0)!=center_water
            ||(g.z>1.0)!=media_only||abs(abs(g.z)-center.z)>0.20 {bilinear=0.0;}
        let difference=center_position-position;
        let error=abs(dot(difference,oct_decode(g.xy)))+length(difference)*0.002;
        if error>tolerance*2.0 {bilinear=0.0;}
        candidates[i]=RayCandidate(r,g,position,bilinear);
        if bilinear>0.0&&error<best {best=error;center_geometry=g;center_sample=q;}
    }
    if abs(center_geometry.w)<=0.0 {return vec4f(0.0);}
    let n=oct_decode(center_geometry.xy);let water=center_geometry.z<0.0;
    var sum=vec3f(0.0);var weights=0.0;
    for(var i=0u;i<4u;i++) {
        let candidate=candidates[i];let g=candidate.geometry;
        if candidate.bilinear<=0.0||(g.w<0.0)!=(center_geometry.w<0.0) {continue;}
        let gn=oct_decode(g.xy);if dot(n,gn)<0.85 {continue;}
        let weight=candidate.bilinear*ray_spatial_weight(center_position,candidate.position,gn,
            center.z,abs(g.z),0.0,tolerance);
        sum+=candidate.radiance.rgb*weight;weights+=weight;
    }
    let basis=select(ray_filter_basis(textureLoad(indirect,p,0)),vec3f(1.0),water||media_only);
    var correction=sum/max(weights,0.00001)*basis;
    // T is never denoised with radiance. The nearest compatible primary retains
    // its signed fallback classification, independently of diffuse smoothing.
    let primary_t=textureLoad(transmission,center_sample,0).r;
    if !media_only && primary_t>0.0 {
        let v=normalize(ray_frame.eye.xyz-center_position);
        var mapped=oct_decode(center.xy);if dot(mapped,v)<0.0 {mapped=-mapped;}
        let fallback=bg_pbr_prefiltered_sky(reflect(-v,mapped),center.z,ray_frame.horizon.xyz,ray_frame.zenith);
        let specular=textureLoad(response,p,0);
        correction-=fallback*specular.rgb*specular.w*primary_t;
    }
    return vec4f(correction,0.0);
}

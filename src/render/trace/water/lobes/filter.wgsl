// Present only in the opted reconstruction shader; default interfaces/resources
// and the existing filter remain unchanged. Dynamic correction is not sampled.
@group(0) @binding(10) var ray_lobe_moments:texture_2d<f32>;
@group(0) @binding(11) var ray_lobe_guide:texture_2d<f32>;
fn ray_lobe_camera_direction(p:vec2i,full_size:vec2i,stride:i32)->vec3f {
    let pixel=min(p*stride+vec2i(stride/2),full_size-vec2i(1));
    let ndc=(vec2f(pixel)+0.5)/vec2f(full_size)*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
    let far=ray_frame.inverse*vec4f(ndc,1.0,1.0);
    return normalize(far.xyz/far.w);
}
fn ray_lobe_variance(m:vec4f)->vec2f {
    // A nonnegative variance removes only roundoff in the moment subtraction;
    // neither the raw sample nor reconstructed radiance is clipped.
    return max(vec2f(m.y-m.x*m.x,m.w-m.z*m.z),vec2f(0.0));
}
fn ray_lobe_luminance_weight(center:vec2f,sample:vec2f,
    center_variance:vec2f,sample_variance:vec2f,center_age:f32,sample_age:f32)->vec2f {
    let difference=center-sample;
    // Variance of each mean plus a relative1% edge floor. No absolute HDR cap:
    // multiplying both illumination lobes by a scalar retains these weights.
    let bandwidth=4.0*(center_variance/center_age+sample_variance/sample_age)
        +0.0001*(center*center+sample*sample);
    var weight=vec2f(1.0);
    for(var i=0u;i<2u;i++) {
        if difference[i]!=0.0 {
            weight[i]=select(0.0,exp(-difference[i]*difference[i]/bandwidth[i]),bandwidth[i]>0.0);
        }
    }
    return weight;
}
fn ray_filter_first_water(p:vec2i,center:vec4f,center_geometry:vec4f)->vec4f {
    let guide=textureLoad(ray_lobe_guide,p,0);let moments=textureLoad(ray_lobe_moments,p,0);
    let variance=ray_lobe_variance(moments);
    let stride=max(2,i32(ray_frame.parameters.z));
    let size=vec2i(textureDimensions(radiance));let full_size=vec2i(textureDimensions(receiver));
    let center_pixel=min(p*stride+vec2i(stride/2),full_size-vec2i(1));
    let center_position=world_position(vec2f(center_pixel),center.a,vec2f(full_size));
    let normal=oct_decode(center_geometry.xy);
    let reflection_direction=reflect(ray_lobe_camera_direction(p,full_size,stride),oct_decode(guide.xy));
    let roughness=guide.z;
    let tolerance=max(0.012,center.a*0.0015);
    // The actual mapped normal, camera direction and roughness bound reflection
    // support. Transmission lacks an authoritative path endpoint and stays3x3.
    let angular_bandwidth=max(0.0000125,2.0*roughness*roughness*roughness*roughness);
    let center_reflection=textureLoad(transmission,p,0).rgb;
    let center_transmission=center.rgb-center_reflection;
    // Center-relative normalized sums preserve the same mathematical weights
    // while constant HDR lobes return their exact center, without clipping.
    var reflection_sum=vec3f(0.0);var transmission_sum=vec3f(0.0);
    var weights=vec2f(0.0);
    for(var y=-2;y<=2;y++) {for(var x=-2;x<=2;x++) {
        let q=p+vec2i(x,y);if any(q<vec2i(0))||any(q>=size) {continue;}
        let g=textureLoad(geometry,q,0);let r=textureLoad(radiance,q,0);
        let sample_guide=textureLoad(ray_lobe_guide,q,0);
        if sample_guide.w<=0.0||r.a<=0.0||!ray_complete_path(g.z)||g.w<=0.0
            ||abs(sample_guide.z-roughness)>0.20 {continue;}
        let gn=oct_decode(g.xy);if dot(normal,gn)<0.85 {continue;}
        let sample_pixel=min(q*stride+vec2i(stride/2),full_size-vec2i(1));
        let sample_position=world_position(vec2f(sample_pixel),r.a,vec2f(full_size));
        let offset=vec2f(q-p);let distance_squared=dot(offset,offset);
        let plane=ray_spatial_weight(center_position,sample_position,gn,
            roughness,sample_guide.z,distance_squared,tolerance);
        let sample_moments=textureLoad(ray_lobe_moments,q,0);
        let statistical=ray_lobe_luminance_weight(moments.xz,sample_moments.xz,
            variance,ray_lobe_variance(sample_moments),guide.w,sample_guide.w);
        let sample_direction=reflect(ray_lobe_camera_direction(q,full_size,stride),oct_decode(sample_guide.xy));
        let angular=max(0.0,1.0-dot(reflection_direction,sample_direction));
        var reflection_weight=0.0;
        if angular<=angular_bandwidth*4.0 {
            reflection_weight=plane*exp(-angular/angular_bandwidth)*statistical.x;
        }
        var transmission_weight=0.0;
        if abs(x)<=1&&abs(y)<=1 {
            transmission_weight=plane*exp(-distance_squared*1.5)*statistical.y;
        }
        let reflected=textureLoad(transmission,q,0).rgb;
        reflection_sum+=(reflected-center_reflection)*reflection_weight;
        transmission_sum+=(r.rgb-reflected-center_transmission)*transmission_weight;
        weights+=vec2f(reflection_weight,transmission_weight);
    }}
    var reflected=center_reflection;var transmitted=center_transmission;
    if weights.x>0.0 {reflected+=reflection_sum/weights.x;}
    if weights.y>0.0 {transmitted+=transmission_sum/weights.y;}
    return vec4f(reflected+transmitted,center.a);
}

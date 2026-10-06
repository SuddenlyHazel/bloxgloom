fn oct_decode(e:vec2f)->vec3f {
    var n=vec3f(e,1.0-abs(e.x)-abs(e.y));let t=max(-n.z,0.0);
    n.x+=select(t,-t,n.x>=0.0);n.y+=select(t,-t,n.y>=0.0);return normalize(n);
}
fn oct_encode(n:vec3f)->vec2f {
    var p=n.xy/(abs(n.x)+abs(n.y)+abs(n.z));
    if n.z<0.0 {p=(vec2f(1.0)-abs(p.yx))*select(vec2f(-1.0),vec2f(1.0),p>=vec2f(0.0));}
    return p;
}
// Raster water owns its explicit class; negative visibility also marks moving
// foliage and must never be inferred from a ray/alpha/depth disagreement.
fn ray_is_water(indirect_alpha:f32)->bool {return indirect_alpha < -1.5;}
// Filter irradiance-like corrections, then remodulate by the center material.
// Exact retained ambient includes albedo, visibility and material AO; a floor keeps dark
// caves, conductor-only surfaces and water finite without dividing by black.
fn ray_filter_basis(indirect:vec4f)->vec3f {
    return max(indirect.rgb*abs(indirect.a),vec3f(0.003));
}
fn ray_history_compatible(old_depth:f32,expected_depth:f32,old_geometry:vec4f,
    normal:vec3f,roughness:f32,water:bool,footprint:f32)->bool {
    if old_depth<=0.0 || old_geometry.w<=0.0 {return false;}
    if (old_geometry.z<0.0)!=water {return false;}
    let depth_tolerance=max(0.04,expected_depth*0.003)+min(footprint,0.25);
    return abs(old_depth-expected_depth)<=depth_tolerance
        && dot(normal,oct_decode(old_geometry.xy))>select(0.95,0.90,water)
        && abs(abs(old_geometry.z)-roughness)<0.16;
}
// The nearest history texel lies on its own camera ray. Radial distances to
// two jittered/grazing samples of the same water plane can differ by metres.
// Reconstruct that exact prior sample ray from the VP rows, then validate its
// stored point against the current geometric interface plane. This keeps
// parallel layers/foreground disocclusions separate without loosening opaque
// receiver history or comparing their unrelated radial distances.
fn ray_water_history_compatible(old_depth:f32,old_geometry:vec4f,normal:vec3f,
    geometry_class:f32,position:vec3f,previous:mat4x4f,previous_eye:vec3f,
    previous_pixel:vec2f,full_size:vec2f,footprint:f32)->bool {
    if old_depth<=0.0||old_geometry.w<=0.0||old_geometry.z>=-2.0 {return false;}
    if abs(old_geometry.z-geometry_class)>=0.16
        ||dot(normal,oct_decode(old_geometry.xy))<=0.90 {return false;}
    let ndc=(previous_pixel+0.5)/full_size*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
    let row0=vec3f(previous[0].x,previous[1].x,previous[2].x);
    let row1=vec3f(previous[0].y,previous[1].y,previous[2].y);
    let row3=vec3f(previous[0].w,previous[1].w,previous[2].w);
    let axis=cross(row0-ndc.x*row3,row1-ndc.y*row3);
    if dot(axis,axis)<0.000000000001 {return false;}
    var direction=normalize(axis);
    if dot(direction,position-previous_eye)<0.0 {direction=-direction;}
    let cosine=dot(direction,normal);
    if abs(cosine)<0.000001 {return false;}
    let expected=dot(position-previous_eye,normal)/cosine;
    if expected<=0.0 {return false;}
    // Convert radial half-float/error tolerance to normal-plane distance.
    // A small geometric floor covers large-coordinate float cancellation;
    // grazing incidence does not expand this into a permissive radial band.
    let tolerance=max(0.04,expected*0.003*abs(cosine))
        +min(footprint,0.25)*abs(cosine);
    return abs(dot(position-previous_eye-direction*old_depth,normal))<=tolerance;
}
fn ray_spatial_weight(center_position:vec3f,sample_position:vec3f,
    sample_normal:vec3f,center_roughness:f32,sample_roughness:f32,
    pixel_distance_squared:f32,plane_tolerance:f32)->f32 {
    // A tangential depth change on one wall/floor is not an edge. A point on
    // another parallel surface, even with the same normal, fails this plane test.
    let plane_error=abs(dot(center_position-sample_position,sample_normal));
    let plane_weight=exp(-pow(plane_error/max(plane_tolerance,0.001),2.0));
    let roughness_weight=exp(-pow((center_roughness-sample_roughness)/0.20,2.0));
    return plane_weight*roughness_weight*exp(-pixel_distance_squared/9.0);
}

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

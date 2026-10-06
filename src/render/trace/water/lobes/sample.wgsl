// Opted-in first-interface bookkeeping only. The legacy function remains an
// independent control. No new traversal, random draw, response or MIS weight.
struct RayWaterLobes {total:vec3f,reflection:vec3f};
fn ray_primary_water_lobes(hit:RayHit,position:vec3f,incoming:vec3f,footprint:vec4f)->RayWaterLobes {
    let boundary=ray_water_interface(hit,incoming,position,footprint);
    let direct=ray_water_component(ray_water_direct(hit,position,incoming,boundary,true),false);
    var result=direct;var reflection=direct;
    for(var branch=0u;branch<2u;branch++) {
        let sample=ray_water_conditional_sample(boundary,incoming,vec2f(random(),random()),branch==1u);
        if all(sample.weight==vec3f(0.0)) {continue;}
        let origin=position+boundary.outward*select(-0.006,0.006,dot(sample.direction,boundary.outward)>=0.0);
        let state=ray_water_at(origin);
        if state<0&&sample.transmitted {continue;}
        let sky=ray_triangle_at(hit.triangle).b.w;
        let contribution=sample.weight*transport_state(origin,sample.direction,sky,512.0,false,vec4f(0.0),1u,sample.pdf,sample.eta*sample.eta);
        result+=contribution;
        if branch==0u {reflection+=contribution;}
    }
    return RayWaterLobes(result,reflection);
}

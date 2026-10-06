// Proposal-only guidance for diffuse vertices under actual water. A retained
// cosine component covers every reflected direction; full-mixture weighting
// preserves the original BRDF and finite-scene occlusion/medium contracts.
struct RayWaterGuide {axis:vec3f,cosine:f32,enabled:bool};
struct RayWaterDiffuseSample {direction:vec3f,weight:f32};
fn ray_water_guide_pdf(normal:vec3f,guide:RayWaterGuide,direction:vec3f)->f32 {
    let cosine=max(dot(normal,direction),0.0)/RAY_PI;
    if !guide.enabled {return cosine;}
    var cone=0.0;
    if dot(guide.axis,direction)>=guide.cosine {cone=1.0/(2.0*RAY_PI*(1.0-guide.cosine));}
    return 0.5*(cosine+cone);
}
fn ray_water_guide_sample(normal:vec3f,guide:RayWaterGuide,xi:vec3f)->RayWaterDiffuseSample {
    let azimuth=2.0*RAY_PI*xi.y;
    var cosine=sqrt(max(0.0,1.0-xi.x));var axis=normal;
    if guide.enabled&&xi.z<0.5 {cosine=mix(1.0,guide.cosine,xi.x);axis=guide.axis;}
    let sine=sqrt(max(0.0,1.0-cosine*cosine));
    let direction=ray_basis(axis)*vec3f(sine*cos(azimuth),sine*sin(azimuth),cosine);
    let pdf=ray_water_guide_pdf(normal,guide,direction);
    var weight=0.0;
    if pdf>0.0 {weight=max(dot(normal,direction),0.0)/(RAY_PI*pdf);}
    return RayWaterDiffuseSample(direction,weight);
}
// Snell differentiates the transmitted direction with respect to the
// interface normal. The air-side normal tilt is compressed by this Jacobian
// in water; the finite solar disc retains its own refracted angular extent.
fn ray_water_guide_angle(roughness:f32,cosine_air:f32)->f32 {
    let cosine_water=sqrt(max(0.0,1.0-(1.0-cosine_air*cosine_air)/(RAY_WATER_IOR*RAY_WATER_IOR)));
    let compression=1.0-cosine_air/(RAY_WATER_IOR*cosine_water);
    let solar=RAY_WATER_SUN_RADIUS/RAY_WATER_IOR;
    return min(4.0*roughness*roughness*compression+solar,0.4);
}
fn ray_water_cone_pdf(guide:RayWaterGuide,direction:vec3f)->f32 {
    if !guide.enabled||dot(guide.axis,direction)<guide.cosine {return 0.0;}
    return 1.0/(2.0*RAY_PI*(1.0-guide.cosine));
}
fn ray_water_cone_direction(guide:RayWaterGuide,xi:vec2f)->vec3f {
    let cosine=mix(1.0,guide.cosine,xi.x);let sine=sqrt(max(0.0,1.0-cosine*cosine));
    let angle=2.0*RAY_PI*xi.y;
    return ray_basis(guide.axis)*vec3f(sine*cos(angle),sine*sin(angle),cosine);
}
// Retain the normalized molecular phase over the entire sphere. Guiding only
// changes its proposal; the original phase divided by the complete mixture
// restores the same physical scattering response for every sampled direction.
fn ray_water_guided_phase(incoming:vec3f,guide:RayWaterGuide,xi:vec3f)->RayWaterDiffuseSample {
    var direction=ray_water_phase_direction(incoming,xi.xy);
    if guide.enabled&&xi.z<0.5 {direction=ray_water_cone_direction(guide,xi.xy);}
    let phase=ray_water_phase(dot(incoming,direction));
    var pdf=phase;
    if guide.enabled {pdf=0.5*(phase+ray_water_cone_pdf(guide,direction));}
    return RayWaterDiffuseSample(direction,phase/pdf);
}
// Querying through ray_cast records real dynamic dependencies. Unknown space
// or an opaque first hit disables the proposal; it never adds sunlight or sky.
const RAY_GUIDE_BOUNDARY_FIRST:bool=true;
// Static geometry supplies an upper bound before coverage traversal. Include
// one more representable distance to detect unknown space beginning exactly
// at the candidate: intersections at the proven prefix are strictly excluded.
// Only the final, proven prefix reaches dynamic queries, retaining dependency
// tracking and nearest-hit ties from the original coverage-first walk.
fn ray_water_guide_hit(origin:vec3f,axis:vec3f)->RayHit {
    if !RAY_GUIDE_BOUNDARY_FIRST {
        let prefix=ray_water_known_distance(origin,axis,512.0);
        return ray_cast(origin,axis,prefix);
    }
    var candidate=ray_cast_static(origin,axis,512.0);
    var bound=512.0;
    if candidate.triangle!=0xffffffffu {
        bound=min(512.0,bitcast<f32>(bitcast<u32>(candidate.distance)+1u));
    }
    let prefix=ray_water_known_distance(origin,axis,bound);
    if candidate.distance>=prefix {
        candidate=RayHit(prefix,0xffffffffu,vec2f(0.0),vec3f(0.0));
    }
    return dynamic_ray_cast(origin,axis,prefix,candidate);
}
fn ray_water_sun_guide(origin:vec3f,fallback:vec3f)->RayWaterGuide {
    let disabled=RayWaterGuide(fallback,1.0,false);
    if !RAY_CAUSTIC_GUIDE||ray_water_at(origin)!=1||ray_frame.sun.y<=0.0
        ||all(ray_frame.solar.xyz==vec3f(0.0)) {return disabled;}
    let sun=normalize(ray_frame.sun.xyz);
    var axis=-refract(-sun,vec3f(0.0,1.0,0.0),1.0/RAY_WATER_IOR);
    var roughness=0.12;var cosine_air=max(sun.y,0.0);
    for(var iteration=0u;iteration<2u;iteration++) {
        let hit=ray_water_guide_hit(origin,axis);
        if !ray_water_is(hit) {return disabled;}
        let point=origin+axis*hit.distance;
        let boundary=ray_water_interface(hit,axis,point,vec4f(0.0));
        if boundary.eta_from<=boundary.eta_to {return disabled;}
        let refracted=-refract(-sun,-boundary.normal,1.0/RAY_WATER_IOR);
        if dot(refracted,refracted)<=0.0 {return disabled;}
        axis=normalize(refracted);roughness=boundary.roughness;
        cosine_air=max(dot(-boundary.normal,sun),0.0);
    }
    // The original lobe remains in every mixture, including its GGX tails.
    // This Snell-compressed cone targets the peak, not a hard response cutoff.
    let angle=ray_water_guide_angle(roughness,cosine_air);
    return RayWaterGuide(axis,cos(angle),true);
}

fn ray_water_diffuse_guide(position:vec3f,normal:vec3f)->RayWaterGuide {
    return ray_water_sun_guide(position+normal*0.006,normal);
}

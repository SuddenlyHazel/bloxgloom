fn oracle_sun_guide(origin:vec3f,fallback:vec3f)->RayWaterGuide {
    let disabled=RayWaterGuide(fallback,1.0,false);
    if !RAY_CAUSTIC_GUIDE||ray_water_at(origin)!=1||ray_frame.sun.y<=0.0
        ||all(ray_frame.solar.xyz==vec3f(0.0)) {return disabled;}
    let sun=normalize(ray_frame.sun.xyz);
    var axis=-refract(-sun,vec3f(0.0,1.0,0.0),1.0/RAY_WATER_IOR);
    var roughness=0.12;var cosine_air=max(sun.y,0.0);
    for(var iteration=0u;iteration<2u;iteration++) {
        let limit=oracle_known_distance(origin,axis,512.0);
        let hit=ray_cast(origin,axis,limit);
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

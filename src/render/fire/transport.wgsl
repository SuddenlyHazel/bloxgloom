// Extinction for post-transport translucent particles. No scattered light is
// added here: the resolved scene already contains primary medium radiance.
// Billboard coverage uses contrast fading, not an exact absorbing sheet: no
// per-particle front-segment in-scattering or multiple scattering is computed.
// The air law and cloud sample partitions match trace/medium.wgsl.
fn bg_particle_air_depth(height:f32,vertical:f32,distance:f32,sigma:f32)->f32 {
    let end=height+vertical*distance;
    if vertical==0.0 {return sigma*exp(-max(height-20.0,0.0)/90.0)*distance;}
    let near=min(height,end);
    let below=clamp((20.0-near)/abs(vertical),0.0,distance);
    let above=distance-below;
    let span=abs(vertical)*above/90.0;
    var average=1.0;
    if span>0.0 {
        var opacity=1.0-exp(-span);
        if span<0.001 {opacity=span*(1.0-span*(0.5-span*(1.0/6.0-span/24.0)));}
        average=opacity/span;
    }
    return sigma*below+sigma*exp(-max(near-20.0,0.0)/90.0)*above*average;
}
fn bg_particle_medium_transmittance(origin:vec3f,world:vec3f,sigma:f32,cloud:vec3f)->f32 {
    let relative=world-origin;let distance=length(relative);
    if distance==0.0 {return 1.0;}
    let direction=relative/distance;
    let interval=bg_cloud_interval(origin,direction,distance);
    if interval.y<=interval.x {return exp(-bg_particle_air_depth(origin.y,direction.y,distance,sigma));}
    var depth=0.0;
    for(var i=0u;i<64u;i++) {
        var step=distance/64.0;var offset=(f32(i)+0.5)*step;
        if i<16u {step=interval.x/16.0;offset=(f32(i)+0.5)*step;}
        else if i<48u {step=(interval.y-interval.x)/32.0;offset=interval.x+(f32(i-16u)+0.5)*step;}
        else {step=(distance-interval.y)/16.0;offset=interval.y+(f32(i-48u)+0.5)*step;}
        let point=origin+direction*offset;
        depth+=(sigma*exp(-max(point.y-20.0,0.0)/90.0)+bg_cloud_density(point,cloud.x,cloud.yz))*step;
    }
    return exp(-depth);
}
fn bg_particle_transmittance(world:vec3f)->f32 {
    if camera.fog_range.w<=0.5 {return 1.0;}
    // Camera horizon.w stores .065*sqrt(weather fog strength). Recover the
    // same strength used by trace's sigma0=.0004+strength*.003 exactly.
    let strength=pow(camera.horizon.w/0.065,2.0);
    return bg_particle_medium_transmittance(camera.eye.xyz,world,0.0004+strength*0.003,camera.cloud.xyz);
}

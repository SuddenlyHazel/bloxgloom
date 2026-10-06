// Shared near/distant water response. Normal ripples leave geometry seams flat.
fn bg_water_waves(world:vec3f,time:f32,footprint:vec4f)->vec3f {
    // Broad wind waves and smaller cross-ripples have different directions,
    // wavelengths and phases, avoiding the two obvious parallel sine grids.
    // Frequencies in time are integral multiples of 0.1, so the shared clock's
    // 20*pi wrap does not introduce a visible jump.
    let spectrum=array<vec4f,8>(
        vec4f(0.44,0.15,0.026,0.2),vec4f(-0.26,0.34,0.023,0.3),
        vec4f(0.10,0.66,0.018,0.4),vec4f(-0.78,0.17,0.014,0.5),
        vec4f(0.36,-1.15,0.012,0.7),vec4f(1.27,0.47,0.009,0.9),
        vec4f(-0.48,1.72,0.006,1.2),vec4f(2.18,-0.29,0.004,1.5));
    let phases=array<f32,8>(0.1,1.8,4.1,0.7,2.6,3.9,1.2,5.4);
    var ripple=vec2f(0.0);
    var lost_variance=0.0;
    // Integrate unresolved sinusoidal slopes over the pixel footprint and
    // remove frequencies before Nyquist; TAA cannot repair spatial aliasing.
    for(var i=0u;i<8u;i++) {
        let wave=spectrum[i];
        let dx=dot(footprint.xy,wave.xy);
        let dy=dot(footprint.zw,wave.xy);
        let retained=exp(-(dx*dx+dy*dy)/24.0)
            *(1.0-smoothstep(2.0,3.141592654,max(abs(dx),abs(dy))));
        if retained>0.0 {
            let phase=dot(world.xz,wave.xy)+time*wave.w+phases[i];
            ripple+=normalize(wave.xy)*sin(phase)*wave.z*retained;
        }
        lost_variance+=wave.z*wave.z*0.5*(1.0-retained*retained);
    }
    return vec3f(ripple,lost_variance);
}

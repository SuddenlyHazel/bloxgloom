// Source MERS/LabPBR SSS is a scattering strength, not measured per-sheet
// optical transmittance. A separate species thickness calibrates absorption.
// Reflected + transmitted diffuse energy is bounded by one per channel;
// Fresnel allocation remains the caller's responsibility.
struct BgFoliageOptics { reflected:vec3f,transmitted:vec3f,share:f32,strength:f32 };
fn bg_foliage_optics(albedo:vec3f,subsurface:f32,flags:u32)->BgFoliageOptics {
    let strength=clamp(max(subsurface,f32((flags>>16u)&255u)/255.0),0.0,1.0);
    let thickness=f32((flags>>27u)&31u)*0.05;
    let share=clamp(0.15+0.65*strength,0.0,0.8);
    let color=clamp(albedo,vec3f(0.005),vec3f(1.0));
    // Beer-Lambert tint over a thin path. Opaque diffuse reflectance is a
    // spectral absorption proxy, not the extinction factor for a whole sheet.
    let transmission_tint=exp(log(color)*max(thickness,0.20));
    return BgFoliageOptics(color*(1.0-share),transmission_tint*share,share,strength);
}
fn bg_foliage_optical_direct(normal:vec3f,plane:vec3f,view:vec3f,sun:vec4f,
    sky:f32,wrap:f32,optics:BgFoliageOptics,solar:vec3f)->vec3f {
    let l=normalize(sun.xyz);
    let n=select(-normal,normal,dot(normal,view)>=0.0);
    let sheet=select(-plane,plane,dot(plane,view)>=0.0);
    let reflected=max((dot(n,l)+wrap)/(1.0+wrap),0.0);
    let forward=0.35+0.65*pow(max(dot(-l,view),0.0),4.0);
    let transmitted=max(-dot(sheet,l),0.0)*forward;
    // Match the path tracer's directional-irradiance units. Wrap/forward
    // shaping remains an artistic thin-sheet lobe, not an extra light source.
    return clamp(sky,0.0,1.0)*(optics.reflected*reflected+optics.transmitted*transmitted)*solar/3.14159265359;
}

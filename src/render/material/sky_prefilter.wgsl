// Binding-free base-sky GGX convolution. All actual material fallback and
// replacement consumers share these nodes and source controls. Clouds belong
// to scene transport; no synthetic ground radiance or solar disc is added.
fn bg_prefiltered_legacy_environment(reflected: vec3f, roughness: f32, horizon: vec3f, zenith: vec4f) -> vec3f {
    // Align quadrature to the sky gradient: azimuth rotation cannot change
    // the convolution, and the pole fallback has vanishing influence.
    var tangent=vec3f(1.0,0.0,0.0);
    if abs(reflected.y)<0.999999 {tangent=normalize(vec3f(0.0,1.0,0.0)-reflected*reflected.y);}
    let bitangent=cross(reflected,tangent);
    let alpha=roughness*roughness;
    let azimuth=array<vec2f,8>(vec2f(1.000000000,0.000000000),vec2f(-0.737368878,0.675490294),vec2f(0.087425725,-0.996171041),vec2f(0.608438860,0.793600752),vec2f(-0.984713485,-0.174181951),vec2f(0.843755296,-0.536728051),vec2f(-0.259604306,0.965715074),vec2f(-0.460907023,-0.887448430));
    var sum=vec3f(0.0);var weights=0.0;
    for(var i=0u;i<8u;i++) {
        let xi=(f32(i)+0.5)/8.0;
        let cosine=sqrt((1.0-xi)/(1.0+(alpha*alpha-1.0)*xi));
        let sine=sqrt(max(0.0,1.0-cosine*cosine));
        let h=tangent*(azimuth[i].x*sine)+bitangent*(azimuth[i].y*sine)+reflected*cosine;
        let ray=reflect(-reflected,h);
        let weight=max(dot(reflected,ray),0.0);
        sum+=(mix(horizon,zenith.xyz,smoothstep(-0.08,0.86,ray.y))*zenith.w*smoothstep(-0.08,0.0,ray.y))*weight;weights+=weight;
    }
    return sum/max(weights,0.00001);
}

fn bg_prefiltered_environment(reflected:vec3f,roughness:f32,horizon:vec3f,zenith:vec4f,
    sun:vec4f,climate:vec2f,source_sky:bool) -> vec3f {
    if !source_sky {return bg_prefiltered_legacy_environment(reflected,roughness,horizon,zenith);}
    // Deterministic up-aligned eight-node approximation. The source sky is
    // anisotropic around the sun; azimuth rotation can change this estimate.
    var tangent=vec3f(1.0,0.0,0.0);
    if abs(reflected.y)<0.999999 {tangent=normalize(vec3f(0.0,1.0,0.0)-reflected*reflected.y);}
    let bitangent=cross(reflected,tangent);
    let alpha=roughness*roughness;
    let azimuth=array<vec2f,8>(vec2f(1.000000000,0.000000000),vec2f(-0.737368878,0.675490294),vec2f(0.087425725,-0.996171041),vec2f(0.608438860,0.793600752),vec2f(-0.984713485,-0.174181951),vec2f(0.843755296,-0.536728051),vec2f(-0.259604306,0.965715074),vec2f(-0.460907023,-0.887448430));
    var sum=vec3f(0.0);var weights=0.0;
    for(var i=0u;i<8u;i++) {
        let xi=(f32(i)+0.5)/8.0;
        let cosine=sqrt((1.0-xi)/(1.0+(alpha*alpha-1.0)*xi));
        let sine=sqrt(max(0.0,1.0-cosine*cosine));
        let h=tangent*(azimuth[i].x*sine)+bitangent*(azimuth[i].y*sine)+reflected*cosine;
        let ray=reflect(-reflected,h);
        let weight=max(dot(reflected,ray),0.0);
        sum+=bg_bsl_sky_default(ray,sun.xyz,sun.w,climate.x,climate.y)*zenith.w*smoothstep(-0.08,0.0,ray.y)*weight;weights+=weight;
    }
    return sum/max(weights,0.00001);
}

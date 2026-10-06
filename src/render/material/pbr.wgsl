// JG RTX's labPBR 1.3 channel contract: src/scripts/labpbr/specular.ts,
// metals.ts. Legacy package companions retain the old smoothness/metalness
// convention. All texture data here is linear; albedo arrives decoded sRGB.
struct BgPbr {
    roughness: f32,
    f0: vec3f,
    metal: f32,
    subsurface: f32,
    porosity: f32,
    emission: f32,
    present: bool,
};

fn bg_conductor_f0(n: vec3f, k: vec3f) -> vec3f {
    return ((n-1.0)*(n-1.0)+k*k) / ((n+1.0)*(n+1.0)+k*k);
}
fn bg_lab_metal_f0(id: u32, albedo: vec3f) -> vec3f {
    var f0 = vec3f(1.0);
    switch id {
        case 230u: { f0 = bg_conductor_f0(vec3f(2.9114,2.9497,2.5845),vec3f(3.0893,2.9318,2.767)); }
        case 231u: { f0 = bg_conductor_f0(vec3f(0.18299,0.42108,1.3734),vec3f(3.4242,2.3459,1.7704)); }
        case 232u: { f0 = bg_conductor_f0(vec3f(1.3456,0.96521,0.61722),vec3f(7.4746,6.3995,5.3031)); }
        case 233u: { f0 = bg_conductor_f0(vec3f(3.1071,3.1812,2.323),vec3f(3.3314,3.3291,3.135)); }
        case 234u: { f0 = bg_conductor_f0(vec3f(0.27105,0.67693,1.3164),vec3f(3.6092,2.6248,2.2921)); }
        case 235u: { f0 = bg_conductor_f0(vec3f(1.91,1.83,1.44),vec3f(3.51,3.4,3.18)); }
        case 236u: { f0 = bg_conductor_f0(vec3f(2.3757,2.0847,1.8453),vec3f(4.2655,3.7153,3.1365)); }
        case 237u: { f0 = bg_conductor_f0(vec3f(0.15943,0.14512,0.13547),vec3f(3.9291,3.19,2.3808)); }
        default: {}
    }
    return clamp(albedo*f0,vec3f(0.0),vec3f(1.0));
}
fn bg_decode_pbr(texel: vec4f, albedo: vec3f, lab: bool, present: bool) -> BgPbr {
    // The perceptual roughness is squared inside the GGX distribution.
    let roughness = max(0.15,1.0-texel.r);
    if !lab {
        let metal = clamp(texel.g,0.0,1.0);
        return BgPbr(roughness,mix(vec3f(0.04),albedo,metal),metal,0.0,0.0,0.0,present);
    }
    let green = u32(round(texel.g*255.0));
    let metal = select(0.0,1.0,green >= 230u);
    let f0 = select(vec3f(texel.g),bg_lab_metal_f0(green,albedo),green >= 230u);
    let blue = texel.b*255.0;
    let sss = select(0.0,clamp((blue-65.0)/190.0,0.0,1.0),blue >= 65.0 && metal == 0.0);
    let porosity = select(0.0,clamp(blue/64.0,0.0,1.0),blue <= 64.0 && metal == 0.0);
    // Worker preparation converts labPBR's alpha=255 "no emission" sentinel
    // to zero before filtering, preventing false glow across mip boundaries.
    let emission = select(0.0,clamp(texel.a*255.0/254.0,0.0,1.0),present && texel.a < 1.0);
    return BgPbr(roughness,clamp(f0,vec3f(0.0),vec3f(1.0)),metal,sss,porosity,emission,present);
}
fn bg_normal_data(texel: vec4f, lab: bool) -> vec4f {
    let xy = texel.rg*2.0-1.0;
    let z = select(texel.b*2.0-1.0,sqrt(max(0.0,1.0-dot(xy,xy))),lab);
    return vec4f(normalize(vec3f(xy,z)),select(1.0,texel.b,lab));
}
fn bg_pbr_diffuse_weight(pbr: BgPbr, nv: f32) -> f32 {
    if !pbr.present { return 1.0; }
    let grazing = pow(1.0-clamp(nv,0.0,1.0),5.0);
    let fresnel = pbr.f0+(max(vec3f(1.0-pbr.roughness),pbr.f0)-pbr.f0)*grazing;
    return clamp((1.0-max(max(fresnel.r,fresnel.g),fresnel.b))*(1.0-pbr.metal),0.0,1.0);
}
fn bg_pbr_sun(normal: vec3f, v: vec3f, sun: vec4f, sky: f32,
    visibility: f32, pbr: BgPbr, radiance: vec3f) -> vec3f {
    if !pbr.present || sky == 0.0 || visibility == 0.0 { return vec3f(0.0); }
    let l = normalize(sun.xyz);
    let h = (l+v)/max(length(l+v),0.0001);
    let nl = max(dot(normal,l),0.0);
    let nv = max(dot(normal,v),0.0);
    let nh = max(dot(normal,h),0.0);
    let vh = max(dot(v,h),0.0);
    let alpha = pbr.roughness*pbr.roughness;
    let a2 = alpha*alpha;
    let denominator = nh*nh*(a2-1.0)+1.0;
    let distribution = a2/max(3.14159265*denominator*denominator,0.00001);
    let k = (pbr.roughness+1.0)*(pbr.roughness+1.0)/8.0;
    let geometry = nl/max(nl*(1.0-k)+k,0.0001) * nv/max(nv*(1.0-k)+k,0.0001);
    let fresnel = pbr.f0+(vec3f(1.0)-pbr.f0)*pow(1.0-vh,5.0);
    let reflection = distribution*geometry*fresnel/max(4.0*nl*nv,0.0001);
    return min(reflection,vec3f(8.0))*nl*radiance*sky*visibility;
}
fn bg_pbr_environment(normal: vec3f, view: vec3f, pbr: BgPbr,
    sky_radiance: vec3f, sky_visibility: f32, local_radiance: vec3f, local_visibility: f32) -> vec3f {
    if !pbr.present { return vec3f(0.0); }
    let nv = clamp(dot(normal,view),0.0,1.0);
    return bg_pbr_environment_weight(nv,pbr.roughness,pbr.f0)*(max(sky_radiance,vec3f(0.0))*clamp(sky_visibility,0.0,1.0)
        +max(local_radiance,vec3f(0.0))*clamp(local_visibility,0.0,1.0));
}

// Analytic split-sum DFG fit: integrates GGX masking/Fresnel across the
// specular lobe instead of applying a single grazing Fresnel to its center.
fn bg_pbr_environment_weight(nv: f32, roughness: f32, f0: vec3f) -> vec3f {
    let r = roughness*vec4f(-1.0,-0.0275,-0.572,0.022)+vec4f(1.0,0.0425,1.04,-0.04);
    let a = min(r.x*r.x,exp2(-9.28*clamp(nv,0.0,1.0)))*r.x+r.y;
    let ab = vec2f(-1.04,1.04)*a+r.zw;
    return clamp(f0*ab.x+vec3f(ab.y),vec3f(0.0),vec3f(1.0));
}
fn bg_pbr_sky(direction: vec3f, horizon: vec3f, zenith: vec4f) -> vec3f {
    return mix(horizon,zenith.xyz,smoothstep(-0.08,0.86,direction.y))*zenith.w*smoothstep(-0.08,0.0,direction.y);
}
// Deterministic GGX importance quadrature of the smooth analytic sky. There
// is no random per-pixel noise and no synthetic ground radiance. SSR uses the
// same function to replace exactly this fallback where real geometry is hit.
fn bg_pbr_prefiltered_sky(reflected: vec3f, roughness: f32, horizon: vec3f, zenith: vec4f) -> vec3f {
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
        sum+=bg_pbr_sky(ray,horizon,zenith)*weight;weights+=weight;
    }
    return sum/max(weights,0.00001);
}

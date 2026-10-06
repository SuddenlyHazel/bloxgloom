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
    // Preset metal albedo tints the entire reflected lobe, including grazing.
    // Generic metallic F0 retains Schlick's untinted white grazing limit.
    reflection_tint: vec3f,
    preset_id: u32,
    // Optional BSL artistic mode uses its squared rational complex Fresnel.
    artistic: bool,
    present: bool,
};

fn bg_conductor_f0(n: vec3f, k: vec3f) -> vec3f {
    return ((n-1.0)*(n-1.0)+k*k) / ((n+1.0)*(n+1.0)+k*k);
}
struct BgConductor {n:vec3f,k:vec3f};
fn bg_conductor_indices(id:u32)->BgConductor {
    let ns=array<vec3f,8>(vec3f(2.9114,2.9497,2.5845),vec3f(0.18299,0.42108,1.3734),
        vec3f(1.3456,0.96521,0.61722),vec3f(3.1071,3.1812,2.3230),
        vec3f(0.27105,0.67693,1.3164),vec3f(1.9100,1.8300,1.4400),
        vec3f(2.3757,2.0847,1.8453),vec3f(0.15943,0.14512,0.13547));
    let ks=array<vec3f,8>(vec3f(3.0893,2.9318,2.7670),vec3f(3.4242,2.3459,1.7704),
        vec3f(7.4746,6.3995,5.3031),vec3f(3.3314,3.3291,3.1350),
        vec3f(3.6092,2.6248,2.2921),vec3f(3.5100,3.4000,3.1800),
        vec3f(4.2655,3.7153,3.1365),vec3f(3.9291,3.1900,2.3808));
    if id>=230u&&id<=237u {return BgConductor(ns[id-230u],ks[id-230u]);}
    return BgConductor(vec3f(0.0),vec3f(1.0));
}
fn bg_lab_metal_f0(id:u32,albedo:vec3f)->vec3f {
    let indices=bg_conductor_indices(id);
    return clamp(albedo*bg_conductor_f0(indices.n,indices.k),vec3f(0.0),vec3f(1.0));
}
// Exact complex-Snell unpolarized conductor Fresnel, expressed entirely in
// real arithmetic (PBRT FrConductor). The preset albedo tint is applied after
// the angular response, including grazing; no Schlick approximation remains.
fn bg_conductor_fresnel(cosine:f32,indices:BgConductor)->vec3f {
    let c=clamp(cosine,0.0,1.0);let c2=c*c;let s2=1.0-c2;
    let n2=indices.n*indices.n;let k2=indices.k*indices.k;
    let t0=n2-k2-s2;
    let magnitude=sqrt(t0*t0+4.0*n2*k2);
    let real_root=sqrt(max(0.5*(magnitude+t0),vec3f(0.0)));
    let t1=magnitude+c2;let t2=2.0*c*real_root;
    let rs=(t1-t2)/max(t1+t2,vec3f(0.000001));
    let t3=c2*magnitude+s2*s2;let t4=t2*s2;
    let rp=rs*(t3-t4)/max(t3+t4,vec3f(0.000001));
    return clamp(0.5*(rs+rp),vec3f(0.0),vec3f(1.0));
}
fn bg_decode_pbr(texel: vec4f, albedo: vec3f, lab: bool, present: bool) -> BgPbr {
    // The perceptual roughness is squared inside the GGX distribution.
    let roughness = max(0.15,1.0-texel.r);
    if !lab {
        let metal = clamp(texel.g,0.0,1.0);
        return BgPbr(roughness,mix(vec3f(0.04),albedo,metal),metal,0.0,0.0,0.0,vec3f(1.0),0u,false,present);
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
    let tint=select(vec3f(1.0),clamp(albedo,vec3f(0.0),vec3f(1.0)),green>=230u&&green<=237u);
    return BgPbr(roughness,clamp(f0,vec3f(0.0),vec3f(1.0)),metal,sss,porosity,emission,tint,select(0u,green,green>=230u&&green<=237u),false,present);
}
fn bg_pbr_fresnel(cosine:f32,pbr:BgPbr)->vec3f {
    if pbr.artistic && pbr.preset_id!=0u {return bg_bsl_advanced_complex_fresnel(cosine,pbr.preset_id);}
    if pbr.preset_id!=0u {return bg_conductor_fresnel(cosine,bg_conductor_indices(pbr.preset_id))*pbr.reflection_tint;}
    return pbr.f0+(pbr.reflection_tint-pbr.f0)*pow(1.0-clamp(cosine,0.0,1.0),5.0);
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
    var geometry = nl/max(nl*(1.0-k)+k,0.0001) * nv/max(nv*(1.0-k)+k,0.0001);
    if pbr.preset_id!=0u {geometry=1.0/(1.0+bg_ggx_lambda(nl,alpha)+bg_ggx_lambda(nv,alpha));}
    var fresnel = bg_pbr_fresnel(vh,pbr);
    if pbr.artistic {
        // BSL direct highlights use SphericalGaussianFresnel with GetMetalCol.
        fresnel=pbr.f0+(1.0-pbr.f0)*exp2((-5.55473*vh-6.98316)*vh);
    }
    let reflection = distribution*geometry*fresnel/max(4.0*nl*nv,0.0001);
    return min(reflection,vec3f(8.0))*nl*radiance*sky*visibility;
}
fn bg_pbr_environment(normal: vec3f, view: vec3f, pbr: BgPbr,
    sky_radiance: vec3f, sky_visibility: f32, local_radiance: vec3f, local_visibility: f32) -> vec3f {
    if !pbr.present { return vec3f(0.0); }
    let nv = clamp(dot(normal,view),0.0,1.0);
    return bg_pbr_material_environment_weight(nv,pbr)*(max(sky_radiance,vec3f(0.0))*clamp(sky_visibility,0.0,1.0)
        +max(local_radiance,vec3f(0.0))*clamp(local_visibility,0.0,1.0));
}

// Analytic split-sum DFG fit: integrates GGX masking/Fresnel across the
// specular lobe instead of applying a single grazing Fresnel to its center.
fn bg_pbr_integrated_ab(nv:f32,roughness:f32)->vec2f {
    let r = roughness*vec4f(-1.0,-0.0275,-0.572,0.022)+vec4f(1.0,0.0425,1.04,-0.04);
    let a = min(r.x*r.x,exp2(-9.28*clamp(nv,0.0,1.0)))*r.x+r.y;
    return vec2f(-1.04,1.04)*a+r.zw;
}
fn bg_pbr_environment_weight(nv:f32,roughness:f32,f0:vec3f)->vec3f {
    let ab=bg_pbr_integrated_ab(nv,roughness);
    return clamp(f0*ab.x+vec3f(ab.y),vec3f(0.0),vec3f(1.0));
}
fn bg_pbr_material_environment_weight(nv:f32,pbr:BgPbr)->vec3f {
    // Source artistic reflections use the view-angle Fresnel directly.
    if pbr.artistic {return bg_pbr_fresnel(nv,pbr);}
    if pbr.preset_id!=0u {return bg_conductor_environment_weight(nv,pbr);}
    let ab=bg_pbr_integrated_ab(nv,pbr.roughness);
    return clamp(pbr.f0*ab.x+pbr.reflection_tint*ab.y,vec3f(0.0),vec3f(1.0));
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

// Named BSL reference helpers. The checked-in settings disable
// ADVANCED_MATERIALS, so its default material path has no packed PBR response.
// Enhanced rendering intentionally keeps the LabPBR path above for GI/relief.
fn bg_bsl_default_material()->BgPbr {
    return BgPbr(1.0,vec3f(0.0),0.0,0.0,0.0,0.0,vec3f(1.0),0u,false,false);
}
// Advanced BSL source oracle, NOT enabled by its default settings. GetMetalCol
// is an artistic baseReflectance distinct from measured conductor normal F0;
// ALBEDO_METAL is disabled. Do not silently substitute this into LabPBR decode.
// Runtime opt-in BSL MATERIAL_FORMAT=1 remap. ALBEDO_METAL is off in the
// source profile: measured preset and reserved metal codes reflect untinted.
// Imported emission alpha has already been sentinel-normalized by the worker.
fn bg_decode_bsl_advanced(texel:vec4f,albedo:vec3f,lab:bool,present:bool)->BgPbr {
    var p=bg_decode_pbr(texel,albedo,lab,present);
    p.artistic=true;
    p.roughness=max(0.025,1.0-texel.r);
    if lab {
        let id=u32(round(texel.g*255.0));
        if texel.g>=0.9 && texel.g<1.0 {
            p.f0=bg_bsl_advanced_preset_base(id);
            p.reflection_tint=vec3f(1.0);
            p.preset_id=id;
        }
        // Source labPBR advanced SSS/porosity remap, including its exact split.
        p.subsurface=select(0.0,clamp(texel.b*1.335-0.355,0.0,1.0),texel.b>0.251);
        p.porosity=select(0.0,texel.b*3.984,texel.b<=0.251);
        p.emission=pow(clamp(texel.a*1.004-0.004,0.0,1.0),2.0);
    }
    return p;
}
fn bg_bsl_advanced_preset_base(id:u32)->vec3f {
    let colors=array<vec3f,8>(vec3f(0.24867,0.22965,0.21366),vec3f(0.88140,0.57256,0.11450),
        vec3f(0.81715,0.82021,0.83177),vec3f(0.27446,0.27330,0.27357),
        vec3f(0.84430,0.48677,0.22164),vec3f(0.36501,0.35675,0.37653),
        vec3f(0.42648,0.37772,0.31138),vec3f(0.91830,0.89219,0.83662));
    if id>=230u&&id<=237u {return colors[id-230u];}
    return vec3f(1.0);
}
fn bg_bsl_advanced_complex_fresnel(cosine:f32,id:u32)->vec3f {
    let indices=bg_conductor_indices(id);let n=indices.n;let k=indices.k;
    let c=clamp(cosine,0.0,1.0);let n2k2=n*n+k*k;let nc=2.0*n*c;
    let rs=(n2k2-nc+c*c)/(n2k2+nc+c*c);
    let rp=(n2k2*c*c-nc+1.0)/(n2k2*c*c+nc+1.0);
    let value=clamp(0.5*(rs+rp),vec3f(0.0),vec3f(1.0));
    return value*value;
}

fn bg_ggx_lambda(cosine:f32,alpha:f32)->f32 {
    let c=max(cosine,0.0001);
    return 0.5*(sqrt(1.0+alpha*alpha*(1.0-c*c)/(c*c))-1.0);
}
// Deterministic visible-normal GGX quadrature. VNDF sampling cancels D and
// the view masking term from BRDF/pdf, leaving F*(1+lambdaV)/(1+lambdaV+lambdaL).
// This stays bounded at grazing and integrates exact conductor angular color.
fn bg_conductor_environment_weight(nv:f32,pbr:BgPbr)->vec3f {
    let cosine=clamp(nv,0.0001,1.0);
    let view=vec3f(sqrt(max(0.0,1.0-cosine*cosine)),0.0,cosine);
    let alpha=pbr.roughness*pbr.roughness;
    let stretched=normalize(vec3f(alpha*view.xy,view.z));
    var tangent=vec3f(1.0,0.0,0.0);
    if stretched.z<0.99999 {tangent=normalize(vec3f(-stretched.y,stretched.x,0.0));}
    let bitangent=cross(stretched,tangent);
    let lambda_view=bg_ggx_lambda(cosine,alpha);
    let azimuth=array<vec2f,16>(vec2f(0.9807852804,0.1950903220),vec2f(-0.9807852804,-0.1950903220),vec2f(-0.1950903220,0.9807852804),vec2f(0.1950903220,-0.9807852804),vec2f(0.5555702330,0.8314696123),vec2f(-0.5555702330,-0.8314696123),vec2f(-0.8314696123,0.5555702330),vec2f(0.8314696123,-0.5555702330),vec2f(0.8314696123,0.5555702330),vec2f(-0.8314696123,-0.5555702330),vec2f(-0.5555702330,0.8314696123),vec2f(0.5555702330,-0.8314696123),vec2f(0.1950903220,0.9807852804),vec2f(-0.1950903220,-0.9807852804),vec2f(-0.9807852804,0.1950903220),vec2f(0.9807852804,-0.1950903220));
    var sum=vec3f(0.0);
    for(var i=0u;i<16u;i++) {
        let disk=sqrt((f32(i)+0.5)/16.0)*azimuth[i];
        let blend=0.5*(1.0+stretched.z);
        let warped_y=mix(sqrt(max(0.0,1.0-disk.x*disk.x)),disk.y,blend);
        let projected_z=sqrt(max(0.0,1.0-disk.x*disk.x-warped_y*warped_y));
        let projected=disk.x*tangent+warped_y*bitangent+projected_z*stretched;
        let half_vector=normalize(vec3f(alpha*projected.xy,max(projected.z,0.000001)));
        let ray=reflect(-view,half_vector);
        if ray.z>0.0 {
            let masking=(1.0+lambda_view)/(1.0+lambda_view+bg_ggx_lambda(ray.z,alpha));
            sum+=bg_pbr_fresnel(max(dot(view,half_vector),0.0),pbr)*masking;
        }
    }
    return clamp(sum/16.0,vec3f(0.0),pbr.reflection_tint);
}

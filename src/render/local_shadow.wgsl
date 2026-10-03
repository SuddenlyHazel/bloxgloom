// Six perspective faces per source, with the same deformed/cutout geometry as
// the sun pass. Only the directional local lobe is shadowed: voxel scattering,
// baked bounce and emission never disappear behind a moving character.
struct BgLocalSource { position_range: vec4f, color_weight: vec4f, faces: array<mat4x4f,6> };
struct BgLocalShadows { params: vec4f, sources: array<BgLocalSource,4> };
@group(0) @binding(4) var<uniform> bg_local_shadows: BgLocalShadows;
@group(0) @binding(5) var bg_local_depth: texture_depth_2d_array;
@group(0) @binding(6) var bg_local_sampler: sampler_comparison;
fn bg_point_visibility(world: vec3f, normal: vec3f, source: BgLocalSource, slot: u32) -> f32 {
    let delta = world-source.position_range.xyz;
    let a = abs(delta);
    var face = select(1u,0u,delta.x >= 0.0);
    if a.y > a.x && a.y >= a.z { face = select(3u,2u,delta.y >= 0.0); }
    else if a.z > a.x && a.z > a.y { face = select(5u,4u,delta.z >= 0.0); }
    // A small world-space normal offset scales with projected texel footprint.
    // It does not grow with camera distance or parallax/normal-map detail.
    let bias = max(0.002, max(a.x,max(a.y,a.z))*bg_local_shadows.params.y*0.65);
    let p = source.faces[face] * vec4f(world+normal*bias,1.0);
    let projected = p.xyz / max(p.w,0.000001);
    let uv = projected.xy*vec2f(0.5,-0.5)+vec2f(0.5);
    if p.w <= 0.0 || projected.z <= 0.0 || projected.z >= 1.0 { return 1.0; }
    // Bilinear comparison filtering needs no derivatives and remains supported
    // by the GL backend, unlike loading depth through a regular sampler.
    return textureSampleCompareLevel(bg_local_depth,bg_local_sampler,uv,i32(slot*6u+face),projected.z-0.00002);
}
fn bg_local_attribution(world:vec3f,radiance:vec3f,upstream:vec3f,source:BgLocalSource)->f32 {
        let delta=source.position_range.xyz-world;
        let d=length(delta);
        // Match only the already-propagated dominant local color/energy. A weak
        // selected red lamp cannot shadow a stronger untracked blue lamp. This
        // estimates attribution; it never adds or attenuates radiance twice.
        let peak=max(radiance.x,max(radiance.y,radiance.z));
        let chroma=radiance/max(peak,0.00001);
        let affinity=pow(max(0.0,1.0-length(chroma-source.color_weight.xyz)),4.0);
        let manhattan=dot(abs(delta),vec3f(1.0));
        let estimate=max(0.0,(source.position_range.w-manhattan+0.5)/15.0);
        let fraction=clamp(estimate*estimate/max(peak,0.00001),0.0,1.0);
        let alignment=max(0.0,dot(upstream,delta/max(d,0.00001)));
        let shadow_range=min(source.position_range.w,bg_local_shadows.params.w);
        let range_fade=1.0-smoothstep(shadow_range*0.85,shadow_range,d);
        return fraction*affinity*alignment*alignment*range_fade;
}
fn bg_shadowed_local_light(world: vec3f, normal: vec3f, radiance: vec3f, direction: vec3f) -> vec3f {
    let confidence = clamp(length(direction),0.0,1.0)*camera.ambient_lower.w;
    let upstream = direction/max(length(direction),0.00001);
    let n = normalize(normal);
    let lambert = max(dot(n,upstream),0.0);
    if confidence <= 0.0 || lambert <= 0.0 || all(radiance <= vec3f(0.0)) {
        return radiance*((1.0-confidence)+confidence*lambert);
    }
    var weight = 0.0;
    var occluded = 0.0;
    for(var slot=0u;slot<4u;slot++) {
        let source=bg_local_shadows.sources[slot];
        if source.color_weight.w <= 0.0 { continue; }
        let w=bg_local_attribution(world,radiance,upstream,source);
        if w <= 0.00001 { continue; }
        weight += w;
        occluded += w*source.color_weight.w*(1.0-bg_point_visibility(world,n,source,slot));
    }
    let visibility=1.0-occluded/max(weight,1.0);
    return radiance*((1.0-confidence)+confidence*lambert*visibility);
}

// Projected shadow motion has no surface velocity. Reject history throughout
// its bounded influence, including uncovered pixels, rather than leaving trails.
// The sign is already reserved for temporal reactivity; AO uses its magnitude.
fn bg_local_history_sign(world:vec3f,normal:vec3f,radiance:vec3f,direction:vec3f)->f32 {
    if camera.ambient_lower.w <= 0.0 || dot(normal,direction) <= 0.0001 { return 1.0; }
    let upstream=direction/max(length(direction),0.00001);
    for(var slot=0u;slot<4u;slot++) {
        let source=bg_local_shadows.sources[slot];
        if source.color_weight.w > 0.0 && bg_local_attribution(world,radiance,upstream,source) > 0.01 { return -1.0; }
    }
    return 1.0;
}

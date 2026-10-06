struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f, cloud: vec4f, ambient_sh:array<vec4f,6> };
struct Tile { relative: vec4f, origin: vec4i };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage,read> coverage: array<vec4i>;
@group(0) @binding(2) var<uniform> options: vec4f;
@group(1) @binding(0) var<uniform> tile: Tile;
@group(2) @binding(0) var material: texture_2d_array<f32>;
@group(2) @binding(1) var material_sampler: sampler;
@group(2) @binding(2) var<storage,read> material_emission:array<f32>;
@group(2) @binding(4) var material_specular:texture_2d_array<f32>;
struct MaterialMetadata { flags: u32, layer: u32 };
@group(2) @binding(5) var<storage, read> material_metadata: array<MaterialMetadata>;
struct In { @location(0) position: vec3f, @location(1) color: u32, @location(2) surface: u32 };
struct Out {
    @builtin(position) position: vec4f,
    @location(0) relative: vec3f,
    @location(1) local: vec3f,
    @location(2) color: vec4f,
    @location(3) sky: f32,
    @location(4) normal: vec3f,
    @location(5) indirect: vec3f,
    @location(6) @interpolate(flat) surface: u32,
    @location(7) light: vec3f,
};
@vertex fn vs_main(v: In) -> Out {
    var normal = vec3f(0.0);
    normal[(v.surface & 7u) / 2u] = select(-1.0, 1.0, (v.surface & 1u) != 0u);
    let sky = f32((v.surface >> 3u) & 15u) / 15.0;
    let glow = f32((v.surface >> 7u) & 15u) / 15.0;
    var o: Out;
    o.relative = v.position+tile.relative.xyz;
    o.position = camera.view_projection*vec4f(o.relative, 1.0);
    o.local = v.position;
    o.normal = normal;
    o.sky = sky;
    o.surface = v.surface;
    o.color = vec4f(vec4u(v.color&255u,(v.color>>8u)&255u,(v.color>>16u)&255u,v.color>>24u))/255.0;
    if (v.surface&2048u)!=0u {
        // Same RGB9E5 decode as the CPU ray representation. Water never uses
        // the opaque material-layer bits, which instead preserve its alpha.
        let scale=exp2(f32(v.color>>27u)-24.0);
        o.color=vec4f(vec3f(vec3u(v.color&511u,(v.color>>9u)&511u,(v.color>>18u)&511u))*scale,
            f32((v.surface>>13u)&255u)/255.0);
    }
    o.light = bg_surface_light(normal, camera.sun, sky, glow*glow*vec3f(1.0,0.57,0.23), vec3f(0.0), vec3f(0.0), 1.0);
    o.indirect = bg_indirect_daylight(normal, camera.sun, sky);
    return o;
}
fn bg_lod_coverage(v: Out) {
    // Ready near coverage includes known empty chunks, in all three dimensions.
    let world = vec3i(floor(v.local-v.normal*0.002))+tile.origin.xyz;
    let k = world >> vec3u(4u);
    var index = (u32(k.x)*73856093u ^ u32(k.y)*19349663u ^ u32(k.z)*83492791u)&16383u;
    for(var probe=0u; probe<16384u; probe++) {
        let c = coverage[index];
        if c.w == 0 { break; }
        if all(c.xyz == k) { discard; }
        index = (index+1u)&16383u;
    }
}
@fragment fn fs_main(v: Out) -> BgSceneOutput {
    bg_lod_coverage(v);
    let axis = (v.surface & 7u)/2u;
    var uv = vec2f(v.local.x, -v.local.y);
    if axis == 0u { uv = vec2f(v.local.z, -v.local.y); }
    if axis == 1u { uv = vec2f(v.local.z, v.local.x); }
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let encoded_layer = (v.surface >> 13u)&0x3ffffu;
    var albedo = v.color.rgb;
    let cutout=(v.surface&4096u)!=0u;
    if encoded_layer!=0u && (cutout||((v.surface&0x80000000u)==0u&&options.y>0.5)) {
        let texel=textureSampleGrad(material,material_sampler,uv,i32(material_metadata[encoded_layer-1u].layer),dx,dy);
        // Coverage remains botanical at every distance, including averaged-color
        // mode; only opaque RGB detail fades to the admitted linear average.
        if cutout && texel.a<0.5 {discard;}
        let detail=1.0-smoothstep(96.0,240.0,length(v.relative));
        albedo=mix(albedo,texel.rgb,detail);
    }
    if BG_BSL_REFERENCE || BG_BSL_ADVANCED_REFERENCE {
        // Coarse summaries retain the authoritative face material identity,
        // while only transition levels sample the original texture artwork.
        let source_albedo=bg_reference_texture_albedo(vec4f(albedo,1.0)).rgb;
        var basic=0.0;
        var emission=0.0;
        if encoded_layer!=0u {
            let flags=material_metadata[encoded_layer-1u].flags;
            if (flags&64u)!=0u {basic=select(1.0,0.5,v.normal.y>0.9999);}
            emission=material_emission[encoded_layer-1u];
            if BG_BSL_REFERENCE {emission=bg_bsl_default_emission(encoded_layer-1u,source_albedo);}
        }
        let receiver=bg_shadow_receiver(v.local+vec3f(tile.origin.xyz));
        let visibility=bg_bsl_reference_sun_visibility(receiver,v.normal,camera.sun.xyz,basic,v.sky);
        let glow=f32((v.surface>>7u)&15u)/15.0;
        let color=bg_bsl_default_surface(source_albedo,v.normal,normalize(-v.relative),
            bg_bsl_reference_relative_lightmap(vec2f(glow,v.sky),v.relative),1.0,basic,emission,visibility,bg_bsl_reference_frame());
        return bg_scene_output(color,vec3f(0.0),v.relative,max(v.sky,camera.eye.w),1.0);
    }
    let world=v.local+vec3f(tile.origin.xyz);
    let view=normalize(-v.relative);
    // Reconstructed ground caps are actual ramps. Their lighting/reflection
    // normal must follow their triangles, while coverage and UVs retain the
    // authoritative voxel face axis. This also matches unchanged flat faces.
    var geometry_normal=v.normal;
    let cross_normal=normalize(cross(dpdx(v.relative),dpdy(v.relative)));
    if !cutout && v.color.a<0.999 {
        geometry_normal=select(-cross_normal,cross_normal,dot(cross_normal,v.normal)>=0.0);
    }
    let normal=select(-geometry_normal,geometry_normal,dot(geometry_normal,view)>=0.0);
    // Coarse summaries retain their actual flat geometry. When source artwork
    // is admitted, decode the same companion channels as near materials;
    // untextured averages never inherit the sentinel layer's material flags.
    var pbr=bg_decode_pbr(vec4f(0.0),albedo,false,false);
    let textured=encoded_layer!=0u && (cutout||((v.surface&0x80000000u)==0u&&options.y>0.5));
    if textured {
        let metadata=material_metadata[encoded_layer-1u];
        let filtered=textureSampleGrad(material_specular,material_sampler,uv,i32(metadata.layer),dx,dy);
        var channels=filtered;
        let lab=(metadata.flags&16u)!=0u;
        if lab {
            let size=textureDimensions(material_specular,0);
            let pixel=min(vec2i(floor(fract(uv)*vec2f(size))),vec2i(size)-vec2i(1));
            let categorical=textureLoad(material_specular,pixel,i32(metadata.layer),0);
            channels=vec4f(filtered.r,categorical.g,categorical.b,filtered.a);
        }
        pbr=bg_decode_pbr(channels,albedo,lab,(metadata.flags&2u)!=0u);
    }
    let receiver=bg_shadow_receiver(world);
    let visibility=bg_sun_visibility(receiver)*bg_primary_sun_transmittance(world);
    let ambient=bg_indirect_daylight(geometry_normal,camera.sun,v.sky);
    let direct=bg_direct_light(geometry_normal,camera.sun,v.sky)*visibility;
    let nv=clamp(dot(normal,view),0.0,1.0);
    let diffuse=bg_pbr_diffuse_weight(pbr,nv);
    let glow=f32((v.surface>>7u)&15u)/15.0;
    let emission=albedo*glow*glow*vec3f(1.0,0.57,0.23);
    let environment=bg_surface_prefiltered_sky(reflect(-view,normal),pbr.roughness);
    let highlight=bg_pbr_sun(normal,view,camera.sun,v.sky,visibility,pbr,bg_sun_radiance())
        +bg_pbr_environment(normal,view,pbr,environment,v.sky,vec3f(0.0),0.0);
    let color=albedo*(vec3f(0.012,0.015,0.022)+ambient+direct)*diffuse+emission+highlight;
    let output=bg_scene_output(color,albedo*ambient*diffuse,v.relative,max(v.sky,camera.eye.w),1.0);
    // Non-PBR surfaces still publish a valid diffuse receiver for primary GI;
    // their reflected fallback weight is zero rather than an invented lobe.
    let response=select(vec3f(0.0),bg_pbr_material_environment_weight(nv,pbr),pbr.present);
    return bg_scene_reflection(output,normal,pbr.roughness,length(v.relative),
        response*bg_fog_transmittance(v.relative,max(v.sky,camera.eye.w)),v.sky);
}
@fragment fn fs_water(v: Out, @builtin(front_facing) front: bool) -> BgSceneOutput {
    let receiver = bg_shadow_receiver(v.local+vec3f(tile.origin.xyz));
    bg_lod_coverage(v);
    let glow = f32((v.surface >> 7u)&15u)/15.0;
    let footprint=vec4f(dpdx(v.local.xz),dpdy(v.local.xz));
    return bg_water_surface(v.color, v.normal, v.sky, glow, v.local+vec3f(tile.origin.xyz), v.relative, front, options.x, bg_sun_visibility(receiver),footprint);
}

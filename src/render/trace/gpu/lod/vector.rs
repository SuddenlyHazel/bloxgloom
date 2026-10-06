//! Explicit lossless page-load experiment; scalar source remains the default.

pub(in crate::render::trace::gpu) fn configured() -> bool {
    let enabled = std::env::var("BLOXGLOOM_GI_LOD_VECTOR_LOAD").as_deref() == Ok("1");
    if enabled && std::env::var("BLOXGLOOM_GI_LOD_ROOT_ORDER").as_deref() == Ok("1") {
        static WARNING: std::sync::Once = std::sync::Once::new();
        WARNING.call_once(||eprintln!("GI LOD vector loads + root ordering: using scalar page decoder; native combined mode failed exact Metal hit validation"));
    }
    enabled
}

pub(in crate::render::trace::gpu) fn source_for(source: &str, vector: bool) -> String {
    if !vector {
        return source.to_owned();
    }
    assert_eq!(
        source.matches("const RAY_LOD_ROOT_ORDER:bool=").count(),
        1,
        "LOD root-order mode anchor changed"
    );
    if source.contains("const RAY_LOD_ROOT_ORDER:bool=true;") {
        // Native combined loads missed a hit in a multi-query Metal entry even
        // though isolated roots/page casts decoded correctly. Preserve the
        // exact proven scalar source for this explicitly unsupported combination.
        return source.to_owned();
    }
    let start = source
        .find("@group(0) @binding(11) var<storage,read> ray_lod_page_0:")
        .expect("LOD page declaration");
    assert_eq!(
        source
            .matches("@group(0) @binding(11) var<storage,read> ray_lod_page_0:")
            .count(),
        1
    );
    let end = source[start..]
        .find("// Static LOD candidates")
        .expect("LOD record helper boundary")
        + start;
    let mut output = source.to_owned();
    output.replace_range(start..end, include_str!("vector.wgsl"));
    // Retain the same geometric arithmetic and accepted-hit clipping/alpha order.
    // Reuse metadata already loaded beside positions; keep packed fields unsigned.
    replace_once(
        &mut output,
        "let a=ray_lod_float3(page,base);\n    let b=ray_lod_float3(page,base+4u);\n    let c=ray_lod_float3(page,base+8u);",
        "let packed_a=ray_lod_quad(page,base);let packed_b=ray_lod_quad(page,base+4u);let packed_c=ray_lod_quad(page,base+8u);\n    let a=bitcast<vec3f>(packed_a.xyz);let b=bitcast<vec3f>(packed_b.xyz);let c=bitcast<vec3f>(packed_c.xyz);",
        1,
    );
    assert_eq!(
        output
            .matches("let flags=ray_lod_word(page,base+19u);")
            .count(),
        2,
        "LOD candidate/opaque flag anchors changed"
    );
    replace_once(
        &mut output,
        "let flags=ray_lod_word(page,base+19u);",
        "let metadata=ray_lod_quad(page,base+16u);let flags=metadata.w;",
        2,
    );
    replace_once(
        &mut output,
        "let uv_c=bitcast<vec2f>(vec2u(ray_lod_word(page,base+16u),ray_lod_word(page,base+17u)));",
        "let uv_c=bitcast<vec2f>(metadata.xy);",
        1,
    );
    replace_once(
        &mut output,
        "let cutout=bitcast<f32>(ray_lod_word(page,base+11u));",
        "let cutout=bitcast<f32>(packed_c.w);",
        1,
    );
    assert_eq!(
        output
            .matches("let material=u32(bitcast<f32>(ray_lod_word(page,base+3u)));")
            .count(),
        2,
        "LOD candidate/opaque material anchors changed"
    );
    replace_once(
        &mut output,
        "let material=u32(bitcast<f32>(ray_lod_word(page,base+3u)));",
        "let material=u32(bitcast<f32>(packed_a.w));",
        2,
    );
    output
}

fn replace_once(source: &mut String, anchor: &str, replacement: &str, count: usize) {
    assert_eq!(
        source.matches(anchor).count(),
        count,
        "vector-load source anchor changed: {anchor}"
    );
    *source = source.replacen(anchor, replacement, 1);
}

#[cfg(test)]
mod tests;

//! Diagnostic selectors are assembled only for explicit cached headless draws.
use super::super::super::shaders;

pub(super) fn filter(optical: bool, family: usize) -> String {
    assert!(family < 3);
    let source = shaders::composition(true, optical);
    if !optical || family == 0 {
        return source;
    }
    let anchor = "    return vec4f(reflected+transmitted,center.a);";
    assert_eq!(source.matches(anchor).count(), 1);
    source.replace(
        anchor,
        if family == 1 {
            "    return vec4f(reflected,center.a);"
        } else {
            "    return vec4f(transmitted,center.a);"
        },
    )
}

pub(super) const FAMILY: &str = r#"
@group(0) @binding(0) var total:texture_2d<f32>;
@group(0) @binding(1) var reflected:texture_2d<f32>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(p*2.0-1.0,0.0,1.0);
}
@fragment fn fs_family(@builtin(position) p:vec4f)->@location(0) vec4f {
 let t=textureLoad(total,vec2i(p.xy),0);let r=textureLoad(reflected,vec2i(p.xy),0);
 return vec4f(FAMILY_RGB,t.a);
}
"#;

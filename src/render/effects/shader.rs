//! Version-two fragment hooks with renderer-owned bindings.
use wgpu::naga;
const STUBS: &str = r#"
fn effect_input(uv: vec2f, index: u32) -> vec4f { return vec4f(0.0); }
fn effect_parameter(index: u32) -> vec4f { return vec4f(0.0); }
fn effect_time() -> f32 { return 0.0; }
fn effect_size() -> vec2f { return vec2f(1.0); }
"#;
pub(super) fn validate(source: &str) -> Result<(), String> {
    let full = format!("{STUBS}\n{source}");
    let module = naga::front::wgsl::parse_str(&full).map_err(|e| e.emit_to_string(&full))?;
    crate::render::hooks::validate(&module)?;
    let function = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some("effect_fragment"))
        .ok_or("missing effect_fragment(vec2f) -> vec4f")?
        .1;
    let vector = |ty, size| matches!(module.types[ty].inner, naga::TypeInner::Vector { size: s, scalar: naga::Scalar { kind: naga::ScalarKind::Float, width: 4 } } if s == size);
    if function.arguments.len() != 1
        || !vector(function.arguments[0].ty, naga::VectorSize::Bi)
        || function
            .result
            .as_ref()
            .is_none_or(|r| !vector(r.ty, naga::VectorSize::Quad))
    {
        return Err("expected effect_fragment(vec2f) -> vec4f".into());
    }
    Ok(())
}
pub(super) fn compose(source: &str) -> String {
    format!(
        r#"
@group(0) @binding(0) var bg_input0: texture_2d<f32>;
@group(0) @binding(1) var bg_sampler: sampler;
struct BgEffectData {{ frame: vec4f, parameters: array<vec4f, 8> }};
@group(0) @binding(2) var<uniform> bg_data: BgEffectData;
@group(0) @binding(3) var bg_input1: texture_2d<f32>;
fn effect_input(uv: vec2f, index: u32) -> vec4f {{
    if index == 0u {{ return textureSampleLevel(bg_input0, bg_sampler, uv, 0.0); }}
    return textureSampleLevel(bg_input1, bg_sampler, uv, 0.0);
}}
fn effect_parameter(index: u32) -> vec4f {{ return bg_data.parameters[min(index, 7u)]; }}
fn effect_time() -> f32 {{ return bg_data.frame.x; }}
fn effect_size() -> vec2f {{ return bg_data.frame.zw; }}
{source}
@fragment fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {{ return effect_fragment(position.xy / bg_data.frame.zw); }}
"#
    )
}

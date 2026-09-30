//! Versioned material hook types, bounded WGSL validation and source generation.
use super::Prepared;
use wgpu::naga;

pub(in crate::render) const TYPES: &str = r#"
struct BgVertex { position: vec3f, normal: vec3f, uv: vec2f };
struct BgSurface { albedo: vec4f, position: vec3f, normal: vec3f, uv: vec2f, light: vec3f, emission: vec3f };
"#;
const STUBS: &str = r#"
fn material_parameter(index: u32) -> vec4f { return vec4f(0.0); }
fn material_texture(uv: vec2f, index: u32) -> vec4f { return vec4f(1.0); }
fn material_time() -> f32 { return 0.0; }
fn material_sun() -> vec3f { return vec3f(0.0, 1.0, 0.0); }
"#;

pub(super) fn validate(source: &str) -> Result<(), String> {
    let full = format!("{TYPES}\n{STUBS}\n{source}");
    let module = naga::front::wgsl::parse_str(&full).map_err(|e| e.emit_to_string(&full))?;
    crate::render::hooks::validate(&module)?;
    let fragment = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some("material_fragment"))
        .ok_or("missing material_fragment(BgSurface) -> BgSurface")?
        .1;
    let check = |f: &naga::Function, name: &str| {
        f.arguments.len() == 1
            && module.types[f.arguments[0].ty].name.as_deref() == Some(name)
            && f.result.as_ref().is_some_and(|r| r.ty == f.arguments[0].ty)
    };
    if !check(fragment, "BgSurface") {
        return Err("expected material_fragment(BgSurface) -> BgSurface".into());
    }
    if let Some((_, vertex)) = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some("material_vertex"))
        && !check(vertex, "BgVertex")
    {
        return Err("expected material_vertex(BgVertex) -> BgVertex".into());
    }
    Ok(())
}

use crate::render::hooks::rename;

pub(super) fn compose(prepared: &Prepared) -> String {
    let mut declarations = String::from(
        r#"
struct BgMaterialData { parameters: array<vec4f, 8>, textures: vec4u };
struct BgVisualData { frame: vec4f, materials: array<BgMaterialData, 16> };
@group(2) @binding(0) var<uniform> bg_visual: BgVisualData;
"#,
    );
    let mut vertex = String::from("fn bg_vertex(input: BgVertex, layer: u32) -> BgVertex {\n");
    let mut fragment = String::from("fn bg_surface(input: BgSurface, layer: u32) -> BgSurface {\n");
    for (index, material) in prepared.materials.iter().enumerate() {
        let prefix = format!("bg_m{index}_");
        let condition = material
            .layers
            .iter()
            .map(|layer| format!("layer == {layer}u"))
            .collect::<Vec<_>>()
            .join(" || ");
        if material.version == 1 {
            let names = std::collections::BTreeSet::from(["custom_albedo".to_string()]);
            declarations.push_str(&rename(&material.shader, &names, &prefix));
            fragment.push_str(&format!("if ({condition}) {{ var result = input; result.albedo = vec4f({prefix}custom_albedo(input.albedo.rgb, input.uv, input.position), input.albedo.a); return result; }}\n"));
        } else {
            let module =
                naga::front::wgsl::parse_str(&format!("{TYPES}\n{STUBS}\n{}", material.shader))
                    .expect("verified hooks");
            let mut names = module
                .functions
                .iter()
                .filter_map(|(_, f)| f.name.clone())
                .collect::<std::collections::BTreeSet<_>>();
            for (_, ty) in module.types.iter() {
                if let Some(name) = &ty.name
                    && name != "BgVertex"
                    && name != "BgSurface"
                {
                    names.insert(name.clone());
                }
            }
            declarations.push_str(&format!(r#"
fn {prefix}material_parameter(index: u32) -> vec4f {{ return bg_visual.materials[{index}].parameters[min(index, 7u)]; }}
fn {prefix}material_texture(uv: vec2f, index: u32) -> vec4f {{ return textureSampleLevel(material, material_sampler, uv, i32(bg_visual.materials[{index}].textures[min(index, {last}u)]), 0.0); }}
fn {prefix}material_time() -> f32 {{ return bg_visual.frame.x; }}
fn {prefix}material_sun() -> vec3f {{ return normalize(camera.sun.xyz); }}
"#, last = material.textures.len() - 1));
            declarations.push_str(&rename(&material.shader, &names, &prefix));
            if names.contains("material_vertex") {
                vertex.push_str(&format!("if ({condition}) {{ var result = {prefix}material_vertex(input); result.position = input.position + clamp(result.position - input.position, vec3f(-{offset}), vec3f({offset})); return result; }}\n", offset = material.vertex_offset));
            }
            fragment.push_str(&format!(
                "if ({condition}) {{ return {prefix}material_fragment(input); }}\n"
            ));
        }
        declarations.push('\n');
    }
    vertex.push_str("return input; }\n");
    fragment.push_str("return input; }\n");
    format!("{declarations}\n{vertex}\n{fragment}")
}

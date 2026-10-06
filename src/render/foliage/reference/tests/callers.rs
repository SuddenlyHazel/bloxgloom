//! Verbatim production caller bodies, selected without process-wide env mutation.
pub(super) fn source() -> String {
    let terrain = include_str!("../../../pipeline.wgsl");
    let camera = terrain
        .lines()
        .find(|line| line.starts_with("struct Camera"))
        .unwrap();
    let start = terrain.find("struct VertexInput {").unwrap();
    let types = &terrain[start..terrain.find("@group(1)").unwrap()];
    let start = terrain.find("fn voxel_vertex(").unwrap();
    let body = &terrain[start..terrain.find("@vertex fn vs_main").unwrap()];
    let reference = body
        .replace("fn voxel_vertex(", "fn reference_vertex(")
        .replace("BG_BSL_REFERENCE || BG_BSL_ADVANCED_REFERENCE", "true");
    let enhanced = body
        .replace("fn voxel_vertex(", "fn enhanced_vertex(")
        .replace("BG_BSL_REFERENCE || BG_BSL_ADVANCED_REFERENCE", "false");
    format!(
        "{}\n{camera}\n{types}\n{reference}\n{enhanced}\n{}",
        crate::render::custom::TYPES,
        r#"
var<private> camera:Camera;
struct MaterialMetadata {flags:u32,layer:u32};
@group(0) @binding(2) var<storage,read> material_map_flags:array<MaterialMetadata>;
fn bg_vertex(v:BgVertex,layer:u32)->BgVertex {return v;}
fn bg_surface_light(n:vec3f,sun:vec4f,sky:f32,emission:vec3f,bounce:vec3f,glow:vec3f,visibility:f32)->vec3f {return vec3f(0.0);}
"#
    )
}

//! Audited default terrain classes for the frozen builtin catalog.
//! Texture identities cannot supply arbitrary Minecraft state; mappings below
//! explicitly describe the builtin artwork/state proxies instead.
use crate::content::{Catalog, TextureDef};
use std::fmt::Write;

pub(super) fn shader(catalog: &Catalog) -> String {
    let mut source = String::from("fn bg_bsl_emission_class(layer:u32)->u32 {switch layer {\n");
    for (id, texture) in catalog.textures().iter().enumerate() {
        let class = texture_class(texture);
        if class != 0 {
            writeln!(source, "case {id}u: {{return {class}u;}}").unwrap();
        }
    }
    source.push_str("default: {return 0u;}\n}}\n");
    source.push_str(include_str!("emission/surface.wgsl"));
    source
}

fn texture_class(texture: &TextureDef) -> u32 {
    let Some(key) = texture.key.strip_prefix("bloxgloom:") else {
        return 0;
    };
    // Frozen engine texture identities, independently audited against the local
    // reference pack. No source-pack parsing or external build dependency.
    match key {
        "jg_glow_lichen"
        | "jg_small_amethyst_bud"
        | "jg_medium_amethyst_bud"
        | "jg_large_amethyst_bud"
        | "jg_amethyst_cluster"
        | "jg_crying_obsidian"
        | "jg_crimson_stem"
        | "jg_crimson_stem_top"
        | "jg_warped_stem"
        | "jg_warped_stem_top" => 150,
        "glowstone" | "jg_magma" | "jg_shroomlight" | "jg_sea_lantern" => 151,
        "jg_cave_vines_head_berries" => 157,
        _ => 0,
    }
}
#[cfg(test)]
mod tests;

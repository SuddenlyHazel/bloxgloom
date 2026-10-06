//! Default terrain classes from the checked-in modern block.properties.
//! Texture identities cannot supply arbitrary Minecraft state; aliases below
//! explicitly describe the builtin artwork/state proxies instead.
use crate::content::{Catalog, TextureDef};
use std::{collections::HashMap, fmt::Write, sync::OnceLock};

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
    let key = key.strip_prefix("jg_").unwrap_or(key);
    // Only these source-state properties are available in these artwork keys.
    // In particular stripped stems, bare cave vines and torchflower do not glow.
    let source_key = match key {
        "magma" => "magma_block",
        "crimson_stem_top" => "crimson_stem",
        "warped_stem_top" => "warped_stem",
        "cave_vines_head_berries" => "cave_vines:berries=true",
        _ => key,
    };
    source_class(source_key)
}

fn source_class(key: &str) -> u32 {
    static CLASSES: OnceLock<HashMap<&'static str, u32>> = OnceLock::new();
    let classes = CLASSES.get_or_init(|| {
        let mapping = include_str!("../../../external/shaders/block.properties")
            .split("#1.13+ Mapping")
            .nth(1)
            .expect("source modern mapping")
            .split("#elif MC_VERSION")
            .next()
            .unwrap();
        let mut result = HashMap::new();
        let mut class = 0;
        for line in mapping.lines() {
            let line = line.trim();
            let entries = if let Some(block) = line.strip_prefix("block.") {
                let Some((id, entries)) = block.split_once('=') else {
                    continue;
                };
                class = id.trim().parse::<u32>().expect("checked-in material class") / 100;
                entries
            } else {
                line
            };
            // Source terrain assigns 159/160 to ores, which are not emitting
            // with the checked-in GLOWING_ORES-off defaults.
            if !(150..=158).contains(&class) {
                continue;
            }
            for entry in entries.split_whitespace() {
                if let Some(name) = entry.strip_prefix("minecraft:") {
                    result.insert(name, class);
                }
            }
        }
        result
    });
    classes.get(key).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests;

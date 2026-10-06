//! Checked-in BSL waving.glsl defaults and explicit builtin material proxies.
use crate::content::{Catalog, TextureDef};
use std::{collections::HashMap, fmt::Write, sync::OnceLock};

pub(in crate::render) fn shader(catalog: &Catalog) -> String {
    let mut source = String::from("fn bg_bsl_wind_class(layer:u32)->u32 {switch layer {\n");
    for (id, texture) in catalog.textures().iter().enumerate() {
        let class = wind_class(texture);
        if class != 0 {
            writeln!(source, "case {id}u: {{return {class}u;}}").unwrap();
        }
    }
    source.push_str("default: {return 0u;}\n}}\n");
    source.push_str(include_str!("reference/waving.wgsl"));
    source
}

// Source Minecraft classes are presentation metadata, not saved block IDs.
// Match frozen builtin texture keys. Generic/package cutouts retain class0.
fn wind_class(texture: &TextureDef) -> u32 {
    if !texture.alpha_cutout {
        return 0;
    }
    let Some(key) = texture.key.strip_prefix("bloxgloom:") else {
        return 0;
    };
    let key = key.strip_prefix("jg_").unwrap_or(key);
    let key = match key {
        "leaves" => "oak_leaves",
        "flower_red" => "poppy",
        "flower_yellow" => "dandelion",
        "flower_blue" => "blue_orchid",
        "tall_grass" => "short_grass",
        "azalea_plant" => "azalea",
        "cave_vines_head_berries" => "cave_vines",
        _ => key,
    };
    let source = source_classes();
    for (suffix, half) in [("_bottom", "lower"), ("_top", "upper")] {
        if let Some(stem) = key.strip_suffix(suffix)
            && let Some(&class) = source.get(format!("{stem}:half={half}").as_str())
        {
            return class;
        }
    }
    let key = if key.starts_with("pitcher_") {
        "pitcher_crop"
    } else {
        key.split("_stage_").next().unwrap_or(key)
    };
    source
        .get(key)
        .copied()
        .or_else(|| {
            // Some source classes carry berry/state properties unavailable on a
            // texture identity. The explicit base-block proxy retains their wind.
            source
                .iter()
                .find_map(|(&name, &class)| (name.split(':').next() == Some(key)).then_some(class))
        })
        .unwrap_or(0)
}

fn source_classes() -> &'static HashMap<&'static str, u32> {
    static CLASSES: OnceLock<HashMap<&'static str, u32>> = OnceLock::new();
    CLASSES.get_or_init(|| {
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
                // Both source berry variants use the same .5/1.25/.06 move.
                if class == 157 {
                    class = 107;
                }
                entries
            } else {
                line
            };
            if !(100..=109).contains(&class) {
                continue;
            }
            for entry in entries.split_whitespace() {
                if let Some(name) = entry.strip_prefix("minecraft:") {
                    result.insert(name, class);
                }
            }
        }
        result
    })
}

#[cfg(test)]
mod tests;

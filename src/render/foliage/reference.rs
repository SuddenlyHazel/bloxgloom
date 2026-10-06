//! Audited BSL waving defaults and explicit builtin material proxies.
use crate::content::{Catalog, TextureDef};
use std::fmt::Write;

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
    // Frozen engine texture identities, independently audited against the local
    // reference pack. No source-pack parsing or external build dependency.
    match key {
        "fern" | "tall_grass" | "jg_short_grass" | "jg_tall_dry_grass" | "jg_bush"
        | "jg_firefly_bush" | "jg_seagrass" | "jg_crimson_roots" | "jg_warped_roots"
        | "jg_nether_sprouts" => 100,
        "flower_red"
        | "flower_yellow"
        | "flower_blue"
        | "jg_dandelion"
        | "jg_poppy"
        | "jg_blue_orchid"
        | "jg_allium"
        | "jg_azure_bluet"
        | "jg_oxeye_daisy"
        | "jg_cornflower"
        | "jg_lily_of_the_valley"
        | "jg_wither_rose"
        | "jg_red_tulip"
        | "jg_orange_tulip"
        | "jg_white_tulip"
        | "jg_pink_tulip" => 101,
        "jg_sunflower_bottom"
        | "jg_lilac_bottom"
        | "jg_rose_bush_bottom"
        | "jg_peony_bottom"
        | "jg_large_fern_bottom"
        | "jg_tall_grass_bottom"
        | "jg_tall_seagrass_bottom" => 102,
        "jg_sunflower_top"
        | "jg_lilac_top"
        | "jg_rose_bush_top"
        | "jg_peony_top"
        | "jg_large_fern_top"
        | "jg_tall_grass_top"
        | "jg_tall_seagrass_top" => 103,
        "jg_torchflower" => 104,
        "leaves"
        | "jg_oak_leaves"
        | "jg_spruce_leaves"
        | "jg_birch_leaves"
        | "jg_jungle_leaves"
        | "jg_acacia_leaves"
        | "jg_dark_oak_leaves"
        | "jg_mangrove_leaves"
        | "jg_cherry_leaves"
        | "jg_pale_oak_leaves"
        | "jg_azalea_leaves"
        | "jg_flowering_azalea_leaves" => 105,
        "jg_vine" | "jg_weeping_vines" | "jg_twisting_vines" => 106,
        "jg_pink_petals"
        | "jg_spore_blossom"
        | "jg_azalea_plant"
        | "jg_cave_vines"
        | "jg_cave_vines_head_berries" => 107,
        "jg_lily_pad" => 108,
        "jg_pitcher_crop_bottom_stage_4"
        | "jg_pitcher_crop_top_stage_4"
        | "jg_dead_bush"
        | "jg_brown_mushroom"
        | "jg_red_mushroom"
        | "jg_sugar_cane"
        | "jg_leaf_litter"
        | "jg_oak_sapling"
        | "jg_spruce_sapling"
        | "jg_birch_sapling"
        | "jg_jungle_sapling"
        | "jg_acacia_sapling"
        | "jg_dark_oak_sapling"
        | "jg_cherry_sapling"
        | "jg_mangrove_propagule"
        | "jg_crimson_fungus"
        | "jg_warped_fungus" => 109,
        _ => 0,
    }
}
#[cfg(test)]
mod tests;

//! Append-only builtin material companions; original texture IDs remain unchanged.
use super::Catalog;
use bloxgloom_host_api::content::Texture;
use std::borrow::Cow;

pub(super) fn register(catalog: &mut Catalog) {
    const MAPS: &[(&str, &[u8])] = &[
        (
            "grass_top_n",
            include_bytes!("../../assets/textures/blocks/grass_top_n.png"),
        ),
        (
            "grass_top_s",
            include_bytes!("../../assets/textures/blocks/grass_top_s.png"),
        ),
        (
            "grass_side_n",
            include_bytes!("../../assets/textures/blocks/grass_side_n.png"),
        ),
        (
            "grass_side_s",
            include_bytes!("../../assets/textures/blocks/grass_side_s.png"),
        ),
        (
            "dirt_n",
            include_bytes!("../../assets/textures/blocks/dirt_n.png"),
        ),
        (
            "dirt_s",
            include_bytes!("../../assets/textures/blocks/dirt_s.png"),
        ),
        (
            "stone_n",
            include_bytes!("../../assets/textures/blocks/stone_n.png"),
        ),
        (
            "stone_s",
            include_bytes!("../../assets/textures/blocks/stone_s.png"),
        ),
        (
            "sand_n",
            include_bytes!("../../assets/textures/blocks/sand_n.png"),
        ),
        (
            "sand_s",
            include_bytes!("../../assets/textures/blocks/sand_s.png"),
        ),
        (
            "snow_n",
            include_bytes!("../../assets/textures/blocks/snow_n.png"),
        ),
        (
            "snow_s",
            include_bytes!("../../assets/textures/blocks/snow_s.png"),
        ),
        (
            "moss_n",
            include_bytes!("../../assets/textures/blocks/moss_n.png"),
        ),
        (
            "moss_s",
            include_bytes!("../../assets/textures/blocks/moss_s.png"),
        ),
        (
            "gravel_n",
            include_bytes!("../../assets/textures/blocks/gravel_n.png"),
        ),
        (
            "gravel_s",
            include_bytes!("../../assets/textures/blocks/gravel_s.png"),
        ),
        (
            "glowstone_n",
            include_bytes!("../../assets/textures/blocks/glowstone_n.png"),
        ),
        (
            "glowstone_s",
            include_bytes!("../../assets/textures/blocks/glowstone_s.png"),
        ),
        (
            "wood_side_n",
            include_bytes!("../../assets/textures/blocks/wood_side_n.png"),
        ),
        (
            "wood_side_s",
            include_bytes!("../../assets/textures/blocks/wood_side_s.png"),
        ),
        (
            "wood_top_n",
            include_bytes!("../../assets/textures/blocks/wood_top_n.png"),
        ),
        (
            "wood_top_s",
            include_bytes!("../../assets/textures/blocks/wood_top_s.png"),
        ),
        (
            "stick_n",
            include_bytes!("../../assets/textures/items/stick_n.png"),
        ),
        (
            "stick_s",
            include_bytes!("../../assets/textures/items/stick_s.png"),
        ),
    ];
    for &(name, png) in MAPS {
        catalog.embedded_texture(&Texture {
            key: format!("bloxgloom:{name}"),
            png: Cow::Borrowed(png),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: false,
            emission_strength: 0.0,
        });
    }
}

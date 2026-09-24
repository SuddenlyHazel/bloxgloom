use super::*;

impl Catalog {
    pub fn builtins() -> Self {
        let mut catalog = Self::new();
        const PNGS: [(&str, &[u8], bool, bool, bool); 20] = [
            (
                "grass_top",
                include_bytes!("../../assets/textures/blocks/grass_top.png"),
                true,
                true,
                false,
            ),
            (
                "grass_side",
                include_bytes!("../../assets/textures/blocks/grass_side.png"),
                true,
                false,
                false,
            ),
            (
                "dirt",
                include_bytes!("../../assets/textures/blocks/dirt.png"),
                true,
                true,
                false,
            ),
            (
                "stone",
                include_bytes!("../../assets/textures/blocks/stone.png"),
                true,
                true,
                false,
            ),
            (
                "sand",
                include_bytes!("../../assets/textures/blocks/sand.png"),
                true,
                true,
                false,
            ),
            (
                "snow",
                include_bytes!("../../assets/textures/blocks/snow.png"),
                true,
                true,
                false,
            ),
            (
                "moss",
                include_bytes!("../../assets/textures/blocks/moss.png"),
                true,
                true,
                false,
            ),
            (
                "gravel",
                include_bytes!("../../assets/textures/blocks/gravel.png"),
                true,
                true,
                false,
            ),
            (
                "glowstone",
                include_bytes!("../../assets/textures/blocks/glowstone.png"),
                true,
                true,
                false,
            ),
            (
                "wood_side",
                include_bytes!("../../assets/textures/blocks/wood_side.png"),
                true,
                true,
                false,
            ),
            (
                "wood_top",
                include_bytes!("../../assets/textures/blocks/wood_top.png"),
                true,
                true,
                false,
            ),
            (
                "leaves",
                include_bytes!("../../assets/textures/foliage/leaves.png"),
                true,
                true,
                false,
            ),
            (
                "flower_red",
                include_bytes!("../../assets/textures/foliage/flower_red.png"),
                false,
                false,
                true,
            ),
            (
                "flower_yellow",
                include_bytes!("../../assets/textures/foliage/flower_yellow.png"),
                false,
                false,
                true,
            ),
            (
                "flower_blue",
                include_bytes!("../../assets/textures/foliage/flower_blue.png"),
                false,
                false,
                true,
            ),
            (
                "fern",
                include_bytes!("../../assets/textures/foliage/fern.png"),
                false,
                false,
                true,
            ),
            (
                "tall_grass",
                include_bytes!("../../assets/textures/foliage/tall_grass.png"),
                false,
                false,
                true,
            ),
            (
                "seeds",
                include_bytes!("../../assets/textures/items/seeds.png"),
                false,
                false,
                true,
            ),
            (
                "sapling",
                include_bytes!("../../assets/textures/items/sapling.png"),
                false,
                false,
                true,
            ),
            (
                "stick",
                include_bytes!("../../assets/textures/items/stick.png"),
                false,
                false,
                true,
            ),
        ];
        for (name, png, stitch_edges, stitch_vertical, alpha_cutout) in PNGS {
            // Embedded assets are exercised by the renderer's material tests; avoid decoding
            // them once here and again during GPU upload on every startup.
            catalog.textures.push(TextureDef {
                key: format!("bloxgloom:{name}").into(),
                png: Cow::Borrowed(png),
                stitch_edges,
                stitch_vertical,
                alpha_cutout,
            });
        }

        let blocks = [
            block(world::AIR, "air", "AIR", [0.0; 4], [0, 0, 0]),
            block(
                world::GRASS,
                "grass",
                "GRASS",
                [0.32, 0.62, 0.26, 1.0],
                [0, 1, 2],
            ),
            block(
                world::DIRT,
                "dirt",
                "DIRT",
                [0.52, 0.34, 0.21, 1.0],
                [2, 2, 2],
            ),
            block(
                world::STONE,
                "stone",
                "STONE",
                [0.48, 0.52, 0.53, 1.0],
                [3, 3, 3],
            ),
            block(
                world::SAND,
                "sand",
                "SAND",
                [0.87, 0.72, 0.43, 1.0],
                [4, 4, 4],
            ),
            block(
                world::SNOW,
                "snow",
                "SNOW",
                [0.86, 0.92, 0.96, 1.0],
                [5, 5, 5],
            ),
            block(
                world::MOSS,
                "moss",
                "MOSS",
                [0.27, 0.47, 0.19, 1.0],
                [6, 6, 6],
            ),
            block(
                world::GRAVEL,
                "gravel",
                "GRAVEL",
                [0.47, 0.43, 0.39, 1.0],
                [7, 7, 7],
            ),
            block(
                world::GLOWSTONE,
                "glowstone",
                "GLOWSTONE",
                [1.0, 0.66, 0.22, 1.0],
                [8, 8, 8],
            ),
            block(
                world::WOOD,
                "wood",
                "WOOD",
                [0.55, 0.34, 0.19, 1.0],
                [10, 9, 10],
            ),
            block(
                world::LEAVES,
                "leaves",
                "LEAVES",
                [0.30, 0.62, 0.34, 1.0],
                [11, 11, 11],
            ),
            block(
                world::RED_FLOWER,
                "red_flower",
                "RED FLOWER",
                [0.86, 0.20, 0.29, 1.0],
                [12, 12, 12],
            ),
            block(
                world::YELLOW_FLOWER,
                "yellow_flower",
                "YELLOW FLOWER",
                [0.96, 0.68, 0.14, 1.0],
                [13, 13, 13],
            ),
            block(
                world::BLUE_FLOWER,
                "blue_flower",
                "BLUE FLOWER",
                [0.33, 0.56, 0.88, 1.0],
                [14, 14, 14],
            ),
            block(
                world::FERN,
                "fern",
                "FERN",
                [0.34, 0.66, 0.37, 1.0],
                [15, 15, 15],
            ),
            block(
                world::TALL_GRASS,
                "tall_grass",
                "TALL GRASS",
                [0.38, 0.69, 0.34, 1.0],
                [16, 16, 16],
            ),
        ];
        for definition in blocks {
            catalog
                .register_block(definition)
                .expect("unique builtin block");
        }
        for id in 1..=world::MAX_BUILTIN_BLOCK {
            let definition = catalog.block(id).unwrap();
            let item = ItemDef {
                id,
                key: definition.key.clone(),
                name: definition.name.clone(),
                swatch: definition.swatch,
                texture: definition.textures.top,
                placeable: Some(id),
                sprite: definition.cutout,
            };
            catalog
                .register_item(item)
                .expect("unique builtin block item");
        }
        for (id, name, label, color, layer) in [
            (
                crate::items::SEEDS,
                "seeds",
                "SEEDS",
                [0.77, 0.52, 0.27, 1.0],
                17,
            ),
            (
                crate::items::SAPLING,
                "sapling",
                "SAPLING",
                [0.33, 0.65, 0.38, 1.0],
                18,
            ),
            (
                crate::items::STICK,
                "stick",
                "STICK",
                [0.61, 0.39, 0.22, 1.0],
                19,
            ),
        ] {
            catalog
                .register_item(ItemDef {
                    id,
                    key: format!("bloxgloom:{name}").into(),
                    name: label.into(),
                    swatch: color,
                    texture: layer,
                    placeable: None,
                    sprite: true,
                })
                .expect("unique builtin item");
        }
        catalog
    }
}

fn block(
    id: BlockId,
    key: &'static str,
    name: &'static str,
    swatch: [f32; 4],
    [top, side, bottom]: [TextureId; 3],
) -> BlockDef {
    let flags = BUILTIN_FLAGS[id as usize];
    BlockDef {
        id,
        key: format!("bloxgloom:{key}").into(),
        name: name.into(),
        swatch,
        textures: BlockTextures { top, side, bottom },
        solid: flags & SOLID != 0,
        opaque: flags & OPAQUE != 0,
        cutout: flags & CUTOUT != 0,
        plant: flags & PLANT != 0,
        replaceable: flags & REPLACEABLE != 0,
        supports_plant: flags & SUPPORTS_PLANT != 0,
        emission: BUILTIN_EMISSION[id as usize],
        reflectance: BUILTIN_REFLECTANCE[id as usize],
    }
}

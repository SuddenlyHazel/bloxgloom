use super::*;
use bloxgloom_host_api::content as api;

impl Catalog {
    pub fn builtins() -> Self {
        let mut catalog = Self::new();
        const PNGS: [(&str, &[u8], bool, bool, bool); 27] = [
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
                false,
                false,
                true,
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
            (
                "kiln_brick",
                include_bytes!("../../assets/textures/blocks/kiln_brick.png"),
                false,
                false,
                false,
            ),
            (
                "kiln_vent",
                include_bytes!("../../assets/textures/blocks/kiln_vent.png"),
                false,
                false,
                false,
            ),
            (
                "kiln_lit",
                include_bytes!("../../assets/textures/blocks/kiln_lit.png"),
                false,
                false,
                false,
            ),
            (
                "hopper_side",
                include_bytes!("../../assets/textures/blocks/hopper_side.png"),
                false,
                false,
                false,
            ),
            (
                "hopper_top",
                include_bytes!("../../assets/textures/blocks/hopper_top.png"),
                false,
                false,
                false,
            ),
            (
                "chest_side",
                include_bytes!("../../assets/textures/blocks/chest_side.png"),
                false,
                false,
                false,
            ),
            (
                "chest_top",
                include_bytes!("../../assets/textures/blocks/chest_top.png"),
                false,
                false,
                false,
            ),
        ];
        for (name, png, stitch_edges, stitch_vertical, alpha_cutout) in PNGS {
            // Embedded assets are exercised by the renderer's material tests; avoid decoding
            // them once here and again during GPU upload on every startup.
            let texture = api::Texture {
                key: format!("bloxgloom:{name}"),
                png: Cow::Borrowed(png),
                stitch_edges,
                stitch_vertical,
                alpha_cutout,
                emission_strength: if name == "glowstone" { 3.5 } else { 0.0 },
            };
            catalog.embedded_texture(&texture);
        }

        super::companions::register(&mut catalog);

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
        for (id, definition) in blocks.into_iter().enumerate() {
            catalog
                .public_block_type_at(BlockTypeId(id as u32), &definition)
                .expect("unique builtin block");
        }
        catalog
            .register_block(BlockDef {
                id: KILN_BLOCK_TYPE,
                key: "bloxgloom:kiln".into(),
                name: "KILN".into(),
                swatch: [0.52, 0.48, 0.43, 1.0],
                textures: BlockTextures {
                    top: TextureId(20),
                    side: TextureId(21),
                    bottom: TextureId(20),
                },
                solid: true,
                opaque: true,
                cutout: false,
                plant: false,
                replaceable: false,
                supports_plant: false,
                flammable: false,
                emission: 0,
                reflectance: [145, 135, 125],
                properties: vec![
                    PropertyDef {
                        name: "facing".into(),
                        values: vec!["north".into(), "east".into(), "south".into(), "west".into()],
                    },
                    PropertyDef {
                        name: "half".into(),
                        values: vec!["lower".into(), "upper".into()],
                    },
                    PropertyDef {
                        name: "lit".into(),
                        values: vec!["false".into(), "true".into()],
                    },
                ],
            })
            .expect("unique builtin kiln block");
        for id in 0..=world::MAX_BUILTIN_BLOCK.0 {
            let block = BlockTypeId(id);
            let properties = if block == BlockTypeId(world::WOOD.0) {
                vec![("axis".to_owned(), "y".to_owned())]
            } else {
                Vec::new()
            };
            catalog
                .register_state(BlockStateId(id), block, properties, None)
                .expect("unique builtin default state");
        }
        for (id, axis) in [(256, "x"), (257, "z")] {
            catalog
                .register_state(
                    BlockStateId(id),
                    BlockTypeId(world::WOOD.0),
                    vec![("axis".to_owned(), axis.to_owned())],
                    None,
                )
                .expect("unique builtin wood orientation");
        }
        for (facing_index, facing) in ["north", "east", "south", "west"].into_iter().enumerate() {
            for half in ["lower", "upper"] {
                for lit in [false, true] {
                    let offset =
                        ((facing_index * 2 + usize::from(half == "upper")) * 2) + usize::from(lit);
                    let id = BlockStateId(KILN_DEFAULT_STATE.0 + offset as u32);
                    let textures = BlockTextures {
                        top: TextureId(20),
                        side: TextureId(if half == "upper" {
                            20
                        } else if lit {
                            22
                        } else {
                            21
                        }),
                        bottom: TextureId(20),
                    };
                    catalog
                        .register_state_with_emission(
                            id,
                            KILN_BLOCK_TYPE,
                            vec![
                                ("facing".to_owned(), facing.to_owned()),
                                ("half".to_owned(), half.to_owned()),
                                ("lit".to_owned(), lit.to_string()),
                            ],
                            Some(textures),
                            Some(if lit { 12 } else { 0 }),
                        )
                        .expect("unique builtin kiln state");
                }
            }
        }
        for id in 1..=world::MAX_BUILTIN_BLOCK.0 {
            let state = BlockStateId(id);
            let definition = catalog.block(state).unwrap();
            let item = api::Item {
                key: definition.key.to_string(),
                name: definition.name.to_string(),
                swatch: definition.swatch,
                texture: catalog
                    .texture(definition.textures.top)
                    .unwrap()
                    .key
                    .to_string(),
                placeable: Some(catalog.state(state).unwrap().key.clone()),
                sprite: definition.cutout,
                drop_size: bloxgloom_host_api::content::DropSize::Normal,
                drop_animation: Default::default(),
                drop_policy: Default::default(),
                components: api::Components::Unstructured,
            };
            catalog
                .public_item_at(ItemId(id), &item)
                .expect("unique builtin block item");
        }
        catalog
            .register_item(ItemDef {
                id: KILN_ITEM,
                key: "bloxgloom:kiln".into(),
                name: "KILN".into(),
                swatch: [0.52, 0.48, 0.43, 1.0],
                texture: TextureId(21),
                placeable: Some(KILN_DEFAULT_STATE),
                sprite: false,
            })
            .expect("unique builtin kiln item");
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
                .public_item_at(
                    id,
                    &api::Item {
                        key: format!("bloxgloom:{name}"),
                        name: label.to_string(),
                        swatch: color,
                        texture: catalog.texture(TextureId(layer)).unwrap().key.to_string(),
                        placeable: None,
                        sprite: true,
                        drop_size: bloxgloom_host_api::content::DropSize::Normal,
                        drop_animation: Default::default(),
                        drop_policy: Default::default(),
                        components: api::Components::Unstructured,
                    },
                )
                .expect("unique builtin item");
        }
        for (id, key, schema_version, schema_fingerprint) in [
            (
                MOSSBUN_ENTITY_TYPE.0,
                "bloxgloom:mossbun",
                MOSSBUN_SCHEMA_VERSION,
                MOSSBUN_SCHEMA_FINGERPRINT,
            ),
            (1, "bloxgloom:drop", 1, 0x4247_454e_0000_0001),
            (2, "bloxgloom:player", 5, 0x4247_454e_0000_0005),
            (
                KILN_ENTITY_TYPE.0,
                "bloxgloom:kiln",
                KILN_SCHEMA_VERSION,
                KILN_SCHEMA_FINGERPRINT,
            ),
        ] {
            catalog
                .register_entity_type(EntityTypeDef {
                    id: EntityTypeId(id),
                    key: key.into(),
                    schema_version,
                    schema_fingerprint,
                })
                .expect("unique builtin entity type");
        }
        catalog
            .register_block(BlockDef {
                id: HOPPER_BLOCK_TYPE,
                key: "bloxgloom:hopper".into(),
                name: "HOPPER".into(),
                swatch: [0.35, 0.39, 0.42, 1.0],
                textures: BlockTextures {
                    top: TextureId(24),
                    side: TextureId(23),
                    bottom: TextureId(23),
                },
                solid: true,
                opaque: true,
                cutout: false,
                plant: false,
                replaceable: false,
                supports_plant: false,
                flammable: false,
                emission: 0,
                reflectance: [100, 110, 120],
                properties: vec![],
            })
            .expect("unique hopper block");
        catalog
            .register_state(HOPPER_STATE, HOPPER_BLOCK_TYPE, vec![], None)
            .expect("unique hopper state");
        catalog
            .register_item(ItemDef {
                id: HOPPER_ITEM,
                key: "bloxgloom:hopper".into(),
                name: "HOPPER".into(),
                swatch: [0.35, 0.39, 0.42, 1.0],
                texture: TextureId(23),
                placeable: Some(HOPPER_STATE),
                sprite: false,
            })
            .expect("unique hopper item");
        catalog
            .register_entity_type(EntityTypeDef {
                id: HOPPER_ENTITY_TYPE,
                key: "bloxgloom:hopper".into(),
                schema_version: 3,
                schema_fingerprint: 0x484f_5050_4552_0003,
            })
            .expect("unique hopper entity");
        let mut chest = catalog.block_type(HOPPER_BLOCK_TYPE).unwrap().clone();
        chest.id = CHEST_BLOCK_TYPE;
        chest.key = "bloxgloom:chest".into();
        chest.name = "CHEST".into();
        chest.swatch = [0.55, 0.33, 0.14, 1.0];
        chest.reflectance = [140, 90, 45];
        chest.textures = BlockTextures {
            top: TextureId(26),
            side: TextureId(25),
            bottom: TextureId(26),
        };
        catalog.register_block(chest).expect("unique chest block");
        catalog
            .register_state(CHEST_STATE, CHEST_BLOCK_TYPE, vec![], None)
            .expect("unique chest state");
        catalog
            .register_item(ItemDef {
                id: CHEST_ITEM,
                key: "bloxgloom:chest".into(),
                name: "CHEST".into(),
                swatch: [0.55, 0.33, 0.14, 1.0],
                texture: TextureId(25),
                placeable: Some(CHEST_STATE),
                sprite: false,
            })
            .expect("unique chest item");
        catalog
            .register_entity_type(EntityTypeDef {
                id: CHEST_ENTITY_TYPE,
                key: "bloxgloom:chest".into(),
                schema_version: 2,
                schema_fingerprint: 0x4348_4553_5400_0002,
            })
            .expect("unique chest entity");
        catalog
            .bind_mobile(
                MOSSBUN_ENTITY_TYPE,
                std::sync::Arc::new(super::creatures::mossbun::definition()),
            )
            .expect("builtin creature");
        catalog.builtin_inventory_screens();
        catalog.builtin_item_icons();
        catalog.builtin_machines();
        for (key, label) in [
            (crate::gameplay::admin::GIVE, "Give"),
            (crate::gameplay::admin::SPAWN, "Spawn"),
            (crate::gameplay::admin::TIME, "Time"),
            (crate::gameplay::admin::WEATHER, "Weather"),
        ] {
            catalog
                .register_action(bloxgloom_host_api::actions::Action {
                    key: key.into(),
                    version: 1,
                    label: label.into(),
                    target: bloxgloom_host_api::actions::Target::Empty,
                    operation: bloxgloom_host_api::actions::Operation::Gameplay,
                    panel: None,
                    command: (!matches!(
                        key,
                        crate::gameplay::admin::TIME | crate::gameplay::admin::WEATHER
                    ))
                    .then(|| bloxgloom_host_api::actions::Command {
                        permission: bloxgloom_host_api::actions::CommandPermission::Admin,
                        aliases: Vec::new(),
                        arguments: if key == crate::gameplay::admin::GIVE {
                            vec![
                                bloxgloom_host_api::actions::CommandArgument::ItemKey {
                                    max_bytes: 128,
                                },
                                bloxgloom_host_api::actions::CommandArgument::Count {
                                    default: Some(128),
                                },
                            ]
                        } else {
                            vec![bloxgloom_host_api::actions::CommandArgument::EntityKey {
                                max_bytes: 128,
                            }]
                        },
                    }),
                })
                .expect("builtin admin action");
            catalog
                .register_gameplay_handler(bloxgloom_host_api::gameplay::HandlerRegistration {
                    key: key.into(),
                    version: 1,
                    event: bloxgloom_host_api::gameplay::EventKind::ActionRequested,
                    target: Some(key.into()),
                    handler: std::sync::Arc::new(crate::gameplay::admin::Admin),
                })
                .expect("builtin admin decision");
        }
        catalog
            .register_action(bloxgloom_host_api::actions::Action {
                key: crate::gameplay::slot_move::KEY.into(),
                version: 1,
                label: "Move slot".into(),
                target: bloxgloom_host_api::actions::Target::Empty,
                operation: bloxgloom_host_api::actions::Operation::Gameplay,
                panel: None,
                command: None,
            })
            .expect("builtin slot move action");
        catalog
            .register_gameplay_handler(bloxgloom_host_api::gameplay::HandlerRegistration {
                key: crate::gameplay::slot_move::KEY.into(),
                version: 1,
                event: bloxgloom_host_api::gameplay::EventKind::ActionRequested,
                target: Some(crate::gameplay::slot_move::KEY.into()),
                handler: std::sync::Arc::new(crate::gameplay::slot_move::SlotMove),
            })
            .expect("builtin slot move decision");
        catalog
            .register_action(bloxgloom_host_api::actions::Action {
                key: crate::gameplay::drop_stack::KEY.into(),
                version: 1,
                label: "Drop stack".into(),
                target: bloxgloom_host_api::actions::Target::Empty,
                operation: bloxgloom_host_api::actions::Operation::Gameplay,
                panel: None,
                command: None,
            })
            .expect("builtin drop stack action");
        catalog
            .register_gameplay_handler(bloxgloom_host_api::gameplay::HandlerRegistration {
                key: crate::gameplay::drop_stack::KEY.into(),
                version: 1,
                event: bloxgloom_host_api::gameplay::EventKind::ActionRequested,
                target: Some(crate::gameplay::drop_stack::KEY.into()),
                handler: std::sync::Arc::new(crate::gameplay::drop_stack::DropStack),
            })
            .expect("builtin drop stack decision");
        catalog
            .register_gameplay_handler(bloxgloom_host_api::gameplay::HandlerRegistration {
                key: "bloxgloom:harvest".into(),
                version: 1,
                event: bloxgloom_host_api::gameplay::EventKind::BlockRemoved,
                target: None,
                handler: std::sync::Arc::new(crate::gameplay::Harvest),
            })
            .expect("builtin harvest handler");
        catalog
            .register_gameplay_handler(bloxgloom_host_api::gameplay::HandlerRegistration {
                key: "bloxgloom:plant_support".into(),
                version: 1,
                event: bloxgloom_host_api::gameplay::EventKind::NeighborChanged,
                target: None,
                handler: std::sync::Arc::new(crate::gameplay::PlantSupport),
            })
            .expect("builtin plant support handler");
        catalog
            .register_gameplay_handler(bloxgloom_host_api::gameplay::HandlerRegistration {
                key: "bloxgloom:pickup".into(),
                version: 1,
                event: bloxgloom_host_api::gameplay::EventKind::PickupRequested,
                target: None,
                handler: std::sync::Arc::new(crate::gameplay::Pickup),
            })
            .expect("builtin pickup decision");
        catalog
    }
}

fn block(
    id: BlockId,
    key: &'static str,
    name: &'static str,
    swatch: [f32; 4],
    [top, side, bottom]: [u32; 3],
) -> api::Block {
    let flags = BUILTIN_FLAGS[id.0 as usize];
    const TEXTURE_KEYS: [&str; 17] = [
        "grass_top",
        "grass_side",
        "dirt",
        "stone",
        "sand",
        "snow",
        "moss",
        "gravel",
        "glowstone",
        "wood_side",
        "wood_top",
        "leaves",
        "flower_red",
        "flower_yellow",
        "flower_blue",
        "fern",
        "tall_grass",
    ];
    api::Block {
        acoustics: None,
        key: format!("bloxgloom:{key}"),
        name: name.into(),
        swatch,
        textures: api::FaceTextures {
            top: format!("bloxgloom:{}", TEXTURE_KEYS[top as usize]),
            side: format!("bloxgloom:{}", TEXTURE_KEYS[side as usize]),
            bottom: format!("bloxgloom:{}", TEXTURE_KEYS[bottom as usize]),
        },
        solid: flags & SOLID != 0,
        material: if flags & OPAQUE != 0 {
            api::Material::Opaque
        } else if flags & CUTOUT != 0 {
            api::Material::Cutout
        } else {
            api::Material::Invisible
        },
        geometry: if id == world::TALL_GRASS {
            api::Geometry::NarrowCrossedPlant
        } else if flags & PLANT != 0 {
            api::Geometry::CrossedPlant
        } else {
            api::Geometry::Cube
        },
        replaceable: flags & REPLACEABLE != 0,
        supports_plant: flags & SUPPORTS_PLANT != 0,
        flammable: flags & FLAMMABLE != 0,
        emission: BUILTIN_EMISSION[id.0 as usize],
        reflectance: BUILTIN_REFLECTANCE[id.0 as usize],
        properties: if id == world::WOOD {
            vec![api::Property {
                name: "axis".into(),
                values: vec!["x".into(), "y".into(), "z".into()],
            }]
        } else {
            Vec::new()
        },
        states: vec![], // Builtin save IDs are reserved explicitly below.
    }
}

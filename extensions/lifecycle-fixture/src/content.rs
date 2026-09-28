//! Independent assets, legal states, foliage selection, item bytes and packages.
use bloxgloom_host_api::{Extension, Registrar, RegistrationError, composition::*, content::*};

pub const LAMP: &str = "fixture:copper_lamp";
pub const REED: &str = "fixture:copper_reed";
pub const CHIP: &str = "fixture:etched_chip";
pub const TEXTURE: &str = "fixture:copper_checks";
pub struct Content;
impl Extension for Content {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
        // Consumer first is intentional: forward dependencies and asset references
        // resolve after collecting the complete bundle, not in callback order.
        r.package(Package {
            key: "fixture:content".into(),
            version: 1,
            dependencies: vec![Dependency {
                package: "fixture:art".into(),
                version: 1,
            }],
            requires: vec![CONTENT.into(), ITEM_ICONS.into()],
        })?;
        r.package(Package {
            key: "fixture:art".into(),
            version: 1,
            dependencies: vec![Dependency {
                package: "bloxgloom:core".into(),
                version: 1,
            }],
            requires: vec![CONTENT.into()],
        })?;
        r.item(Item {
            key: CHIP.into(),
            name: "ETCHED CHIP".into(),
            swatch: [0.8, 0.4, 0.2, 1.0],
            texture: TEXTURE.into(),
            placeable: None,
            sprite: true,
            drop_size: bloxgloom_host_api::content::DropSize::Normal,
            drop_animation: Default::default(),
            components: Components::Opaque {
                version: 2,
                fingerprint: 0xc011,
                max_bytes: 16,
                required: true,
            },
        })?;
        r.item_icon(bloxgloom_host_api::icon::ItemIcon {
            item: CHIP.into(),
            rows: vec![
                ".cccc.".into(),
                "cggggc".into(),
                "cgccgc".into(),
                "cggggc".into(),
                ".cccc.".into(),
            ],
            palette: vec![(b'c', [0.8, 0.4, 0.2, 1.0]), (b'g', [1.0, 0.8, 0.4, 1.0])],
        })?;
        let mut lamp = Block {
            key: LAMP.into(),
            name: "COPPER LAMP".into(),
            swatch: [0.8, 0.4, 0.2, 1.0],
            textures: FaceTextures {
                top: TEXTURE.into(),
                side: "bloxgloom:wood_side".into(),
                bottom: TEXTURE.into(),
            },
            geometry: Geometry::Cube,
            material: Material::Opaque,
            solid: true,
            replaceable: false,
            supports_plant: true,
            flammable: false,
            emission: 0,
            reflectance: [190, 110, 55],
            properties: vec![
                Property {
                    name: "lit".into(),
                    values: vec!["false".into(), "true".into()],
                },
                Property {
                    name: "axis".into(),
                    values: vec!["x".into(), "y".into(), "z".into()],
                },
            ],
            states: vec![],
        };
        for axis in ["z", "x", "y"] {
            for lit in [true, false] {
                lamp.states.push(BlockState {
                    properties: vec![
                        ("lit".into(), lit.to_string()),
                        ("axis".into(), axis.into()),
                    ],
                    textures: lit.then(|| FaceTextures::uniform("bloxgloom:glowstone")),
                    emission: Some(if lit { 13 } else { 0 }),
                });
            }
        }
        r.block(lamp)?;
        r.item(Item {
            key: LAMP.into(),
            name: "COPPER LAMP".into(),
            swatch: [0.8, 0.4, 0.2, 1.0],
            texture: TEXTURE.into(),
            placeable: Some(format!("{LAMP}[axis=y,lit=false]")),
            sprite: false,
            drop_size: bloxgloom_host_api::content::DropSize::Normal,
            drop_animation: Default::default(),
            components: Components::None,
        })?;
        r.block(Block {
            key: REED.into(),
            name: "COPPER REED".into(),
            swatch: [0.4, 0.7, 0.3, 1.0],
            textures: FaceTextures::uniform(TEXTURE),
            geometry: Geometry::NarrowCrossedPlant,
            material: Material::Cutout,
            solid: false,
            replaceable: true,
            supports_plant: false,
            flammable: true,
            emission: 2,
            reflectance: [80, 170, 90],
            properties: vec![],
            states: vec![BlockState::default()],
        })?;
        r.item(Item {
            key: REED.into(),
            name: "COPPER REED".into(),
            swatch: [0.4, 0.7, 0.3, 1.0],
            texture: TEXTURE.into(),
            placeable: Some(REED.into()),
            sprite: true,
            drop_size: bloxgloom_host_api::content::DropSize::Normal,
            drop_animation: Default::default(),
            components: Components::None,
        })?;
        r.tag(Tag {
            key: "fixture:etched".into(),
            kind: TagKind::Item,
            members: vec![TagMember::Definition(CHIP.into())],
        })?;
        r.tag(Tag {
            key: "fixture:copper".into(),
            kind: TagKind::Item,
            members: vec![
                TagMember::Tag("fixture:etched".into()),
                TagMember::Definition(LAMP.into()),
            ],
        })?;
        r.tag(Tag {
            key: "fixture:copper".into(),
            kind: TagKind::Block,
            members: vec![
                TagMember::Definition(LAMP.into()),
                TagMember::Definition(REED.into()),
            ],
        })?;
        r.texture(Texture {
            key: TEXTURE.into(),
            png: PNG.into(),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: true,
            emission_strength: 1.25,
        })
    }
}

// Self-contained 2x2 RGBA PNG: copper checks with transparent alternate texels.
const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0,
    0, 0, 114, 182, 13, 36, 0, 0, 0, 21, 73, 68, 65, 84, 120, 156, 99, 184, 80, 106, 240, 223, 97,
    67, 42, 3, 3, 136, 0, 113, 0, 68, 247, 7, 147, 191, 251, 23, 217, 0, 0, 0, 0, 73, 69, 78, 68,
    174, 66, 96, 130,
];

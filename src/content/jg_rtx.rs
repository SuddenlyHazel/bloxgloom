//! Append-only imported natural and building content. The checked-in manifest
//! owns numeric identities; texture companions never act as placeable blocks.
use super::*;
use bloxgloom_host_api::content as api;
use serde::Deserialize;

mod assets;
mod icons;
#[cfg(test)]
mod tests;

const BLOCK_BASE: u32 = 1024;
const STATE_BASE: u32 = 4096;
const STATE_STRIDE: u32 = 4;

#[derive(Deserialize)]
struct Import {
    textures: Vec<ImportedTexture>,
    blocks: Vec<ImportedBlock>,
}
#[derive(Deserialize)]
struct ImportedTexture {
    key: String,
    alpha_cutout: bool,
    #[serde(default)]
    stitch_edges: bool,
    #[serde(default)]
    stitch_vertical: bool,
    #[serde(default)]
    emission_strength: f32,
    #[serde(default)]
    foliage: ImportedFoliage,
}
#[derive(Default, Deserialize)]
struct ImportedFoliage {
    wrap: f32,
    transmission: f32,
}
#[derive(Deserialize)]
struct ImportedBlock {
    id: u32,
    key: String,
    name: String,
    top: String,
    side: String,
    bottom: String,
    kind: Kind,
    supports_plant: bool,
    flammable: bool,
    emission: u8,
    swatch: [f32; 4],
    reflectance: [u8; 3],
    top_half: Option<String>,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Cube,
    Log,
    Leaves,
    Plant,
    TallPlant,
}

fn import() -> &'static Import {
    static IMPORT: OnceLock<Import> = OnceLock::new();
    IMPORT.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/jg-rtx/catalog.json"))
            .expect("checked-in JG RTX import manifest")
    })
}

pub(super) fn register(catalog: &mut Catalog) {
    let import = import();
    let metadata: HashMap<_, _> = import
        .textures
        .iter()
        .map(|t| (t.key.as_str(), t))
        .collect();
    for &(name, png) in assets::TEXTURES {
        let t = metadata.get(name);
        catalog.embedded_texture(&api::Texture {
            key: format!("bloxgloom:{name}"),
            png: Cow::Borrowed(png),
            stitch_edges: t.is_some_and(|t| t.stitch_edges),
            stitch_vertical: t.is_some_and(|t| t.stitch_vertical),
            alpha_cutout: t.is_some_and(|t| t.alpha_cutout),
            emission_strength: t.map_or(0.0, |t| t.emission_strength),
            foliage: t.map_or(Default::default(), |t| api::FoliageShading {
                wrap: t.foliage.wrap,
                transmission: t.foliage.transmission,
            }),
        });
    }
    // Every imported specular companion uses labPBR. Original replacement
    // materials also have companion maps registered before this append range.
    let imported_bases = import
        .textures
        .iter()
        .filter_map(|t| t.key.strip_suffix("_s"));
    let legacy_replacements = [
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
    ];
    for name in imported_bases.chain(legacy_replacements) {
        if let Ok(base) = catalog.texture_key(&format!("bloxgloom:{name}")) {
            catalog.mark_lab_pbr_texture(base);
        }
    }
    for block in &import.blocks {
        register_block(catalog, block);
    }
}

pub(super) fn register_icons(catalog: &mut Catalog) {
    icons::register(catalog);
}

fn register_block(catalog: &mut Catalog, b: &ImportedBlock) {
    let key = format!("bloxgloom:{}", b.key);
    let texture = |name: &str| catalog.texture_key(&format!("bloxgloom:{name}")).unwrap();
    let textures = BlockTextures {
        top: texture(&b.top),
        side: texture(&b.side),
        bottom: texture(&b.bottom),
    };
    let top_half = b.top_half.as_deref().map(texture);
    let plant = matches!(b.kind, Kind::Plant | Kind::TallPlant);
    let properties = match b.kind {
        Kind::Log => vec![PropertyDef {
            name: "axis".into(),
            values: vec!["x".into(), "y".into(), "z".into()],
        }],
        Kind::TallPlant => vec![PropertyDef {
            name: "half".into(),
            values: vec!["lower".into(), "upper".into()],
        }],
        _ => vec![],
    };
    let id = BlockTypeId(BLOCK_BASE + b.id);
    let default = BlockStateId(STATE_BASE + STATE_STRIDE * b.id);
    catalog
        .register_block(BlockDef {
            id,
            key: key.clone().into(),
            name: b.name.clone().into(),
            swatch: b.swatch,
            textures,
            solid: !plant,
            opaque: !plant && b.kind != Kind::Leaves,
            fluid: false,
            cutout: plant || b.kind == Kind::Leaves,
            plant,
            replaceable: plant,
            supports_plant: b.supports_plant,
            flammable: b.flammable,
            emission: b.emission,
            sky_attenuation: if b.kind == Kind::Leaves { 2 } else { 0 },
            reflectance: b.reflectance,
            properties,
        })
        .expect("unique imported block with stable ID");
    let states = match b.kind {
        Kind::Log => vec![("axis", "y"), ("axis", "x"), ("axis", "z")],
        Kind::TallPlant => vec![("half", "lower"), ("half", "upper")],
        _ => vec![("", "")],
    };
    for (offset, (property, value)) in states.into_iter().enumerate() {
        let properties = if property.is_empty() {
            vec![]
        } else {
            vec![(property.to_owned(), value.to_owned())]
        };
        let override_textures = if value == "upper" {
            let upper = top_half.expect("tall plant upper texture");
            Some(BlockTextures {
                top: upper,
                side: upper,
                bottom: upper,
            })
        } else {
            None
        };
        catalog
            .register_state(
                BlockStateId(default.0 + offset as u32),
                id,
                properties,
                override_textures,
            )
            .expect("unique imported legal state");
    }
    catalog
        .register_item(ItemDef {
            id: ItemId(BLOCK_BASE + b.id),
            key: key.into(),
            name: b.name.clone().into(),
            swatch: b.swatch,
            texture: if plant { textures.side } else { textures.top },
            placeable: Some(default),
            sprite: plant,
        })
        .expect("unique imported finite block item");
}

/// Recognize paired natural plants by their schema, preserving world remapping.
pub(crate) fn tall_pair(
    catalog: &Catalog,
    state: BlockStateId,
) -> Option<(BlockStateId, BlockStateId)> {
    let s = catalog.state(state)?;
    if s.flags & PLANT == 0 || !s.properties.iter().any(|(k, _)| k == "half") {
        return None;
    }
    Some((
        catalog.state_with_property(state, "half", "lower")?,
        catalog.state_with_property(state, "half", "upper")?,
    ))
}

#[inline]
pub(crate) fn is_leaf(catalog: &Catalog, state: BlockStateId) -> bool {
    let flags = catalog.block_flags(state);
    flags & (SOLID | CUTOUT | PLANT) == SOLID | CUTOUT
        && catalog
            .block(state)
            .is_some_and(|b| b.key == "bloxgloom:leaves" || b.key.ends_with("_leaves"))
}

#[inline]
pub(crate) fn is_log(catalog: &Catalog, state: BlockStateId) -> bool {
    catalog.block_flags(state) & FLAMMABLE != 0
        && catalog
            .state(state)
            .is_some_and(|s| s.properties.iter().any(|(property, _)| property == "axis"))
}

/// Only species with an imported sapling produce one on leaf decay.
pub(crate) fn sapling_for_leaf(leaf: &str) -> Option<String> {
    static SAPLINGS: OnceLock<HashSet<String>> = OnceLock::new();
    let keys = SAPLINGS.get_or_init(|| {
        import()
            .blocks
            .iter()
            .filter(|b| b.key.ends_with("_sapling"))
            .map(|b| format!("bloxgloom:{}", b.key))
            .collect()
    });
    let sapling = format!("{}_sapling", leaf.strip_suffix("_leaves")?);
    keys.contains(&sapling).then_some(sapling)
}

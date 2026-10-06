//! Frozen IDs shared by builtin chunk generation and exact distant sampling.
use crate::{content, world::BlockId};
use std::sync::OnceLock;

pub(super) fn state(key: &str) -> BlockId {
    content::catalog()
        .state_by_key(key)
        .unwrap_or_else(|| panic!("missing generation state {key}"))
}

pub(super) struct Palette {
    pub trees: [(BlockId, BlockId); 9],
    pub branch_wood: [(BlockId, BlockId); 9],
    pub rocks: [BlockId; 7],
    pub deep_rock: BlockId,
    pub soils: [BlockId; 4],
    pub flowers: [BlockId; 8],
    pub ores: [BlockId; 8],
    pub deep_ores: [BlockId; 8],
}

pub(super) fn palette() -> &'static Palette {
    static PALETTE: OnceLock<Palette> = OnceLock::new();
    PALETTE.get_or_init(|| Palette {
        trees: [
            "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry",
            "pale_oak",
        ]
        .map(|species| {
            (
                state(&format!("bloxgloom:{species}_log[axis=y]")),
                state(&format!("bloxgloom:{species}_leaves")),
            )
        }),
        branch_wood: [
            "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry",
            "pale_oak",
        ]
        .map(|species| {
            (
                state(&format!("bloxgloom:{species}_wood[axis=x]")),
                state(&format!("bloxgloom:{species}_wood[axis=z]")),
            )
        }),
        rocks: [
            "granite", "diorite", "andesite", "tuff", "calcite", "basalt", "stone",
        ]
        .map(|name| {
            // Basalt is a directional mineral cube, with a vertical default state.
            let catalog = content::catalog();
            catalog
                .state_by_key(&format!("bloxgloom:{name}"))
                .or_else(|| catalog.state_by_key(&format!("bloxgloom:{name}[axis=y]")))
                .expect("registered generation rock")
        }),
        deep_rock: state("bloxgloom:deepslate"),
        soils: ["coarse_dirt", "podzol", "rooted_dirt", "packed_mud"]
            .map(|name| state(&format!("bloxgloom:{name}"))),
        flowers: [
            "poppy",
            "dandelion",
            "blue_orchid",
            "cornflower",
            "allium",
            "oxeye_daisy",
            "lily_of_the_valley",
            "red_tulip",
        ]
        .map(|name| state(&format!("bloxgloom:{name}"))),
        ores: [
            "coal", "iron", "copper", "gold", "lapis", "redstone", "diamond", "emerald",
        ]
        .map(|name| state(&format!("bloxgloom:{name}_ore"))),
        deep_ores: [
            "coal", "iron", "copper", "gold", "lapis", "redstone", "diamond", "emerald",
        ]
        .map(|name| state(&format!("bloxgloom:deepslate_{name}_ore"))),
    })
}

/// Regional rock and broad horizontal strata retain identical absolute-coordinate
/// samples across every chunk face. The immutable world bottom stays legacy stone.
pub(super) fn rock(x: i64, y: i64, z: i64, column: super::Column, seed: u64) -> BlockId {
    // One small ellipsoidal deposit per absolute-coordinate cell. A bounded
    // arithmetic mask is cheaper than adding several full 3D noise fields.
    const CELL: i64 = 12;
    let vein = super::lattice_hash(
        seed ^ 0x01ae_5712,
        x.div_euclid(CELL),
        y.div_euclid(CELL),
        z.div_euclid(CELL),
    );
    let dx = x.rem_euclid(CELL) - 3 - ((vein >> 8) % 6) as i64;
    let dy = y.rem_euclid(CELL) - 3 - ((vein >> 16) % 6) as i64;
    let dz = z.rem_euclid(CELL) - 3 - ((vein >> 24) % 6) as i64;
    let ore = ((vein >> 32) % 8) as usize;
    let max_y = [32, 40, 24, 8, 16, 0, -8, 24][ore];
    if y <= max_y && dx * dx + dy * dy * 2 + dz * dz <= 5 {
        return if y < -32 {
            palette().deep_ores[ore]
        } else {
            palette().ores[ore]
        };
    }
    if y < -32 {
        return palette().deep_rock;
    }
    let layer = (y + column.strata_offset).div_euclid(12);
    let hash = super::lattice_hash(seed ^ 0x930e_6113, column.rock_region, layer, 0);
    // Keep stone as the common substrate; geological bands fill about half.
    if hash.is_multiple_of(3) {
        return crate::world::STONE;
    }
    palette().rocks[(hash % 7) as usize]
}

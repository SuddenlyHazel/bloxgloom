use super::*;
use crate::world::{WOOD, generate_chunk, world_to_chunk};

#[test]
fn imported_tree_materials_match_chunks_and_distant_columns_at_seams() {
    let seed = 0xB10C_6100;
    let tree = (-20..=20)
        .flat_map(|z| (-20..=20).map(move |x| (x, z)))
        .filter_map(|(x, z)| tree_anchor(x, z, seed))
        .find(|tree| tree.log != WOOD && tree.x.rem_euclid(16) >= 13)
        .expect("imported tree crossing a chunk boundary");
    let mut chunks = HashMap::new();
    let mut lod = LodSampler::new(seed);
    let mut logs = 0;
    let mut leaves = 0;
    for z in tree.z - TREE_RADIUS..=tree.z + TREE_RADIUS {
        for x in tree.x - TREE_RADIUS..=tree.x + TREE_RADIUS {
            let bottom = tree.ground_y as i32;
            let top = tree.trunk_top as i32 + 3;
            let distant = lod.column(x, z, bottom, top);
            for y in bottom..top {
                let (key, local) = world_to_chunk(x as i32, y, z as i32);
                let chunk = chunks
                    .entry(key)
                    .or_insert_with(|| generate_chunk(key, seed));
                let sample = generated_block(x, i64::from(y), z, seed);
                assert_eq!(chunk.block(local), Some(sample));
                assert_eq!(distant[(y - bottom) as usize], sample);
                logs += usize::from(sample == tree.log);
                leaves += usize::from(sample == tree.leaves);
            }
        }
    }
    assert!(logs > 0 && leaves > 0);
}

#[test]
fn regional_rock_and_ore_are_registered_and_reproducible() {
    let seed = 0xB10C_6100;
    let column = terrain_column(-16, 16, seed);
    let catalog = crate::content::catalog();
    let mut seen = std::collections::HashSet::new();
    for z in -24..24 {
        for x in -24..24 {
            for y in (-60..24).step_by(2) {
                let block = materials::rock(x, y, z, column, seed);
                assert_eq!(block, materials::rock(x, y, z, column, seed));
                assert!(catalog.state(block).is_some());
                seen.insert(block);
            }
        }
    }
    assert!(seen.contains(&materials::palette().deep_rock));
    assert!(
        materials::palette()
            .ores
            .iter()
            .any(|ore| seen.contains(ore))
    );
    assert!(
        materials::palette()
            .deep_ores
            .iter()
            .any(|ore| seen.contains(ore))
    );
    assert_eq!(generated_block(-16, i64::from(BEDROCK_Y), 16, seed), STONE);
}

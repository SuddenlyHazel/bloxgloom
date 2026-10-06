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
    let mut boughs = 0;
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
                boughs += usize::from(tree.is_log(sample) && sample != tree.log);
            }
        }
    }
    assert!(logs > 0 && leaves > 0 && boughs > 0);
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

#[test]
fn regional_landforms_have_coasts_plateaus_peaks_and_all_climates() {
    for seed in [0xB10C_6100, 17, 99] {
        let mut climates = [false; 5];
        let mut min = i64::MAX;
        let mut max = i64::MIN;
        let mut steep = 0;
        let mut ocean = 0;
        let mut high_flat = 0;
        let mut sampler = hydrology::Sampler::new(seed);
        for z in (-2048..=2048).step_by(32) {
            for x in (-2048..=2048).step_by(32) {
                let column = sampler.column(x, z);
                min = min.min(column.height);
                max = max.max(column.height);
                steep += usize::from(column.rocky);
                ocean += usize::from(column.water_kind == Some(hydrology::Kind::Ocean));
                high_flat += usize::from(column.height > 40 && column.slope < 0.2);
                climates[column.biome as usize] = true;
                assert!(column.height <= i64::from(crate::world::MAX_TERRAIN_HEIGHT));
                assert_eq!(column.height, terrain_column(x, z, seed).height);
            }
        }
        assert!(
            climates.into_iter().all(|present| present),
            "climates for seed {seed}"
        );
        assert!(
            min < 16 && max > 55,
            "landform range {min}..{max} for seed {seed}"
        );
        assert!(
            steep > 20 && ocean > 20 && high_flat > 20,
            "seed {seed}: steep={steep}, ocean={ocean}, high_flat={high_flat}"
        );
        eprintln!(
            "landforms seed={seed}: range={min}..{max} rocky={steep} ocean={ocean} plateaus={high_flat}"
        );
    }
}

#[test]
fn regional_trees_have_openings_species_and_branched_bounded_crowns() {
    let seed = 0xB10C_6100;
    let mut seen = std::collections::HashSet::new();
    let mut occupied = 0;
    let mut branched = 0;
    for z in -80..=80 {
        for x in -80..=80 {
            if let Some(tree) = tree_anchor(x, z, seed) {
                seen.insert(tree.log);
                occupied += 1;
                assert!(tree.trunk_top + 2 <= i64::from(MAX_GENERATED_HEIGHT));
                assert_eq!(
                    tree_piece(tree, tree.x + TREE_RADIUS + 1, tree.trunk_top, tree.z),
                    None
                );
                branched += usize::from((tree.ground_y + 1..=tree.trunk_top).any(|y| {
                    tree_piece(tree, tree.x + 1, y, tree.z).is_some_and(|block| tree.is_log(block))
                        || tree_piece(tree, tree.x, y, tree.z + 1)
                            .is_some_and(|block| tree.is_log(block))
                }));
            }
        }
    }
    assert!(seen.len() >= 6, "regional species: {seen:?}");
    assert!(occupied > 500 && occupied < 13_000, "tree sites={occupied}");
    assert!(branched > 100, "branched trees={branched}");
}

#[test]
#[ignore = "run with --release --ignored --nocapture for isolated generation timing"]
fn generation_cost_report() {
    // Wall time is reported, never asserted: shared CI/build contention should
    // not make deterministic behavior flaky. Use --release --nocapture to measure.
    let seed = 0xB10C_6100;
    let start = std::time::Instant::now();
    let mut count = 0;
    let mut checksum = 0u64;
    for z in -2..=2 {
        for x in -2..=2 {
            for y in [-2, 0, 1, 3] {
                let chunk = generate_blocks(ChunkKey { x, y, z }, seed);
                checksum =
                    checksum.wrapping_add(chunk.iter().map(|id| u64::from(id.0)).sum::<u64>());
                count += 1;
            }
        }
    }
    eprintln!(
        "generation: {count} chunks {:.3} ms/chunk, checksum={checksum}",
        start.elapsed().as_secs_f64() * 1000.0 / f64::from(count)
    );
    assert_ne!(checksum, 0);
}

#[test]
fn generated_landscape_capture_sites_match_their_ecological_subjects() {
    let seed = 0xB10C_6100;
    let coast = terrain_column(-656, -2048, seed);
    assert_eq!(coast.water_kind, Some(hydrology::Kind::Ocean));
    assert!(coast.height > 8 && coast.height < landforms::SEA_LEVEL);
    let meadow = terrain_column(-2000, -2048, seed);
    assert_eq!(meadow.biome, Biome::Plains);
    assert!(meadow.water_level.is_none() && meadow.height > 20);
    let mountain = terrain_column(-1024, -1840, seed);
    assert_eq!(mountain.biome, Biome::Highland);
    assert!(mountain.height > 60);
    let (x, z) = (-710i64, -2044i64);
    let grove = tree_anchor(
        x.div_euclid(trees::TREE_CELL),
        z.div_euclid(trees::TREE_CELL),
        seed,
    )
    .expect("the fixed cherry grove capture contains a generated tree");
    assert_eq!(grove.log, materials::palette().trees[7].0);
    assert_eq!(terrain_column(grove.x, grove.z, seed).biome, Biome::Forest);
}

use super::*;
use crate::world::{ChunkKey, generate_chunk, terrain, world_to_chunk};
use std::collections::{HashMap, HashSet};

const SEED: u64 = 0xB10C_6100;

fn cold_column() -> Column {
    let mut column = terrain::base_column(-1024, -2208, SEED);
    column.biome = terrain::Biome::Tundra;
    column.height = 24;
    column.temperature = -0.55;
    column.slope = 0.15;
    column.rocky = false;
    column.shore = false;
    column.water_level = None;
    column
}

#[test]
fn colder_high_ground_retains_snow_and_exposed_slopes_shed_it() {
    let mild = Column {
        temperature: -0.24,
        ..cold_column()
    };
    let cold = Column {
        temperature: -0.88,
        ..mild
    };
    let high = Column { height: 88, ..cold };
    let exposed = Column {
        slope: 1.5,
        rocky: true,
        ..cold
    };
    let mut counts = [0; 4];
    let mut bare = HashSet::new();
    for z in (-256..256).step_by(3) {
        for x in (-256..256).step_by(3) {
            let states = [mild, cold, high, exposed].map(|c| surface(x, z, c, SEED));
            for (count, state) in counts.iter_mut().zip(states) {
                *count += usize::from(state == SNOW);
            }
            assert!(!(states[0] == SNOW && states[1] != SNOW));
            assert!(!(states[1] == SNOW && states[2] != SNOW));
            assert!(!(states[3] == SNOW && states[1] != SNOW));
            bare.insert(surface(x, z, mild, SEED));
        }
    }
    assert!(counts[0] > 100 && counts[1] > counts[0] && counts[2] > counts[1]);
    assert!(counts[3] > 0 && counts[3] < counts[1]);
    assert!(bare.is_superset(&HashSet::from([
        SNOW,
        GRAVEL,
        STONE,
        materials::palette().soils[0]
    ])));
}

#[test]
fn snow_and_bare_ground_are_not_locked_to_wfc_or_chunk_perimeters() {
    let column = cold_column();
    for seed in [SEED, 17, 99] {
        let mut snow = [0usize; 2];
        let mut total = [0usize; 2];
        let mut transitions = [0usize; 2];
        let mut edges = [0usize; 2];
        for z in -384_i64..384 {
            for x in (-384_i64..384).step_by(2) {
                let border = usize::from(
                    x.rem_euclid(32) < 4
                        || x.rem_euclid(32) >= 28
                        || z.rem_euclid(32) < 4
                        || z.rem_euclid(32) >= 28,
                );
                let state = surface(x, z, column, seed);
                snow[border] += usize::from(state == SNOW);
                total[border] += 1;
                // A true drift boundary is as likely at an ordinary coordinate
                // as at a chunk/WFC boundary; no forced region-edge mask.
                let boundary = usize::from(x.rem_euclid(16) == 0);
                transitions[boundary] += usize::from(state != surface(x - 1, z, column, seed));
                edges[boundary] += 1;
            }
        }
        let coverage = std::array::from_fn::<_, 2, _>(|i| snow[i] as f64 / total[i] as f64);
        let rate = std::array::from_fn::<_, 2, _>(|i| transitions[i] as f64 / edges[i] as f64);
        assert!(
            coverage.iter().all(|f| (0.2..0.95).contains(f)),
            "{seed}: {coverage:?}"
        );
        assert!(
            (coverage[0] - coverage[1]).abs() < 0.08,
            "perimeter bias: {seed} {coverage:?}"
        );
        assert!(
            (rate[0] - rate[1]).abs() < 0.015,
            "seam transitions: {seed} {rate:?}"
        );
        eprintln!(
            "snow seed={seed}: interior/border coverage={coverage:?}, ordinary/seam transitions={rate:?}"
        );
    }
}

#[test]
fn cold_top_ignores_wfc_state_but_preserves_authoritative_shore_and_subsoil() {
    let column = cold_column();
    let mut seen = HashSet::new();
    for z in (-256..256).step_by(7) {
        for x in (-256..256).step_by(7) {
            let states = std::array::from_fn::<_, 4, _>(|pattern| {
                terrain::generated_block_with_pattern(
                    x,
                    column.height,
                    z,
                    column,
                    pattern as u8,
                    SEED,
                )
            });
            assert!(states.iter().all(|state| *state == states[0]));
            seen.insert(states[0]);
            // Shore treatment must still win over cold drift and rocky flags.
            let shore = Column {
                shore: true,
                rocky: true,
                water_level: Some(column.height),
                ..column
            };
            assert_eq!(
                terrain::generated_block_with_pattern(x, column.height, z, shore, 0, SEED),
                GRAVEL
            );
            assert_eq!(
                terrain::generated_block_with_pattern(x, column.height - 1, z, shore, 0, SEED),
                terrain::DIRT
            );
        }
    }
    assert!(seen.contains(&SNOW) && seen.contains(&GRAVEL) && seen.contains(&STONE));
}

#[test]
fn other_biomes_keep_their_existing_surface_palettes() {
    // Independent version-8 palettes remain exact. Use the existing wet-column
    // cavern guard to test the surface rule rather than randomly exposed caves.
    let soils = materials::palette().soils;
    for (biome, expected) in [
        (
            terrain::Biome::Plains,
            [terrain::GRASS, terrain::GRASS, soils[0], STONE],
        ),
        (
            terrain::Biome::Forest,
            [terrain::GRASS, soils[1], terrain::MOSS, soils[2]],
        ),
        (
            terrain::Biome::Desert,
            [terrain::SAND, terrain::SAND, GRAVEL, STONE],
        ),
        (terrain::Biome::Highland, [STONE, GRAVEL, GRAVEL, SNOW]),
    ] {
        let column = Column {
            biome,
            temperature: 0.2,
            water_level: Some(24),
            ..cold_column()
        };
        for (pattern, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                terrain::generated_block_with_pattern(-32, 24, -16, column, pattern as u8, SEED),
                expected
            );
        }
    }
}

#[test]
fn projected_cold_coast_matches_direct_chunks_and_lod_across_negative_seams() {
    let mut lod = terrain::LodSampler::new(SEED);
    let mut chunks = HashMap::new();
    let mut cold = 0;
    for z in [-2241, -2240, -2239, -2209, -2208, -2207] {
        for x in [-1057, -1056, -1055, -1025, -1024, -1023, -993, -992, -991] {
            let column = terrain::terrain_column(x, z, SEED);
            cold += usize::from(column.biome == terrain::Biome::Tundra);
            let bottom = column.height as i32 - 3;
            let top = column.height as i32 + 3;
            let distant = lod.column(x, z, bottom, top);
            for y in bottom..top {
                let (key, local) = world_to_chunk(x as i32, y, z as i32);
                let chunk = chunks
                    .entry(key)
                    .or_insert_with(|| generate_chunk(key, SEED));
                let exact = terrain::generated_block(x, i64::from(y), z, SEED);
                assert_eq!(chunk.block(local), Some(exact), "{x},{y},{z}");
                assert_eq!(distant[(y - bottom) as usize], exact, "LOD {x},{y},{z}");
            }
        }
    }
    assert_eq!(
        cold, 54,
        "the actual far-coast regression crosses cold negative seams"
    );
}

#[test]
#[ignore = "isolated CPU generation timings; run release with --ignored --nocapture"]
fn cold_surface_generation_cost_report() {
    // These are two representative version-9 workloads, not a before/after
    // comparison: retain a version-8 binary for any generation regression claim.
    for (label, origin) in [("warm", [0, 0]), ("cold-coast", [-64, -140])] {
        let started = std::time::Instant::now();
        let mut checksum = 0u64;
        for z in origin[1]..origin[1] + 4 {
            for x in origin[0]..origin[0] + 4 {
                for y in [0, 1] {
                    let blocks = terrain::generate_blocks(ChunkKey { x, y, z }, SEED);
                    checksum =
                        checksum.wrapping_add(blocks.iter().map(|id| u64::from(id.0)).sum::<u64>());
                }
            }
        }
        eprintln!(
            "{label}: 32 chunks {:.3}ms/chunk checksum={checksum}",
            started.elapsed().as_secs_f64() * 1000.0 / 32.0
        );
        assert_ne!(checksum, 0);
    }
}

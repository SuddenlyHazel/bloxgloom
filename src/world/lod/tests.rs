use super::*;
use crate::content::OPAQUE;
use crate::world::{AIR, ChunkKey, STONE, WOOD};

#[test]
fn sampler_matches_authoritative_chunks_including_negative_coordinates() {
    let mut sampler = LodSampler::new(17);
    for (x, z) in [(-33, -17), (0, 0), (91, 37), (257, -145)] {
        let values = sampler.column(x, z, -64, 80);
        for y in -4..5 {
            let key = ChunkKey {
                x: (x as i32).div_euclid(16),
                y,
                z: (z as i32).div_euclid(16),
            };
            let blocks = crate::world::terrain::generate_blocks(key, 17);
            for ly in 0..16 {
                let index = Chunk::index([
                    (x as i32).rem_euclid(16) as usize,
                    ly,
                    (z as i32).rem_euclid(16) as usize,
                ])
                .unwrap();
                assert_eq!(
                    values[(y * 16 + ly as i32 + 64) as usize],
                    blocks[index],
                    "at{x},{},{z}",
                    y * 16 + ly as i32
                );
            }
        }
    }
}
#[test]
fn high_off_center_saved_structures_override_coarse_sampling() {
    let catalog = Catalog::builtins();
    let key = TileKey {
        level: 4,
        x: 0,
        z: 0,
    };
    let mut blocks = vec![AIR; 16usize.pow(3)];
    blocks[Chunk::index([0, 1, 0]).unwrap()] = WOOD;
    let overlay = Chunk::from_blocks(ChunkKey { x: 0, y: 10, z: 0 }, 1, blocks);
    let tile = build(key, 2, 1, &catalog, &[overlay]).unwrap();
    let column = &tile.columns[0];
    assert!(column.known(160, 176));
    assert!(
        column
            .spans
            .iter()
            .any(|s| s.bottom <= 161 && s.top >= 162 && s.state == WOOD)
    );
    assert!(!column.known(80, 160));
}
#[test]
fn coarse_tiles_are_deterministic_and_dark_beneath_top_surface() {
    let catalog = Catalog::builtins();
    let key = TileKey {
        level: 4,
        x: -1,
        z: 0,
    };
    let first = builtin_lod_tile(key, 1, 1, &catalog).unwrap();
    assert_eq!(first, builtin_lod_tile(key, 1, 1, &catalog).unwrap());
    first.validate(&catalog).unwrap();
    assert!(
        first
            .columns
            .iter()
            .any(|c| c.spans.iter().any(|s| s.state == STONE && s.sky == 0))
    );
    for c in &first.columns {
        for (i, s) in c.spans.iter().enumerate() {
            if c.spans[i + 1..]
                .iter()
                .any(|roof| catalog.state(roof.state).unwrap().flags & OPAQUE != 0)
            {
                assert_eq!(s.sky, 0);
            }
        }
    }
}

#[test]
fn routine_distant_skyline_tiles_fit_payload_budget() {
    let catalog = Catalog::builtins();
    for seed in [1, 17, 44] {
        for (x, z) in [(-2, -2), (-1, 0), (0, 0), (1, -1), (2, 2)] {
            let key = TileKey { level: 4, x, z };
            let tile = builtin_lod_tile(key, 1, seed, &catalog)
                .unwrap_or_else(|error| panic!("seed{seed} tile{x},{z}: {error}"));
            println!(
                "builtin seed{seed} tile{x},{z}: {} bytes {} spans",
                tile.encoded_bytes(),
                tile.columns.iter().map(|c| c.spans.len()).sum::<usize>()
            );
            assert!(tile.encoded_bytes() <= crate::lod::MAX_TILE_BYTES);
            assert!(tile.geometric_error >= 144);
        }
    }
}

#[test]
fn transition_sampling_retains_off_center_canopies_and_shared_vertical_air() {
    let catalog = Catalog::builtins();
    let mut sampler = LodSampler::new(17);
    let occupied = |id| {
        catalog.state(id).is_some_and(|s| {
            id != AIR
                && s.flags
                    & (crate::content::OPAQUE | crate::content::CUTOUT | crate::content::FLUID)
                    != 0
                && s.flags & crate::content::PLANT == 0
        })
    };
    let tree = (-16..16)
        .flat_map(|z| (-16..16).map(move |x| (x, z)))
        .find_map(|(x, z)| crate::world::terrain::tree_anchor(x, z, 17))
        .expect("tree fixture");
    let mut found = false;
    for z in ((tree.z - 4).div_euclid(2) * 2..=tree.z + 4).step_by(2) {
        for x in ((tree.x - 4).div_euclid(2) * 2..=tree.x + 4).step_by(2) {
            let fine = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .map(|(dx, dz)| sampler.column(x + dx, z + dz, -64, 80));
            if !(0..144).any(|y| fine[3][y] == AIR && fine.iter().any(|c| c[y] == tree.leaves)) {
                continue;
            }
            let merged =
                sampling::cell(&mut sampler, x as i32, z as i32, 2, -64, 80, &catalog).unwrap();
            for y in -64..80 {
                let expected = fine.iter().any(|c| occupied(c[(y + 64) as usize]));
                assert_eq!(
                    merged.get(&y).is_some_and(|id| occupied(*id)),
                    expected,
                    "at {x},{y},{z}"
                );
            }
            let c = sampling::column(
                merged,
                vec![Interval {
                    bottom: -64,
                    top: 80,
                }],
                &catalog,
            )
            .unwrap();
            for (i, s) in c.spans.iter().enumerate() {
                if c.spans[i + 1..]
                    .iter()
                    .any(|roof| catalog.state(roof.state).unwrap().flags & OPAQUE != 0)
                {
                    assert_eq!(s.sky, 0);
                }
            }
            found = true;
            break;
        }
        if found {
            break;
        }
    }
    assert!(
        found,
        "fixture must include a canopy missed by center sampling"
    );
}

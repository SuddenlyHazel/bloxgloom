use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn test_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-world-{}-{stamp}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn negative_and_boundary_coordinates() {
    assert_eq!(
        world_to_chunk(-1, -16, -17),
        (
            ChunkKey {
                x: -1,
                y: -1,
                z: -2
            },
            [15, 0, 15]
        )
    );
    assert_eq!(
        world_to_chunk(16, 15, 0),
        (ChunkKey { x: 1, y: 0, z: 0 }, [0, 15, 0])
    );
    assert_eq!(world_to_chunk(i32::MIN, 0, i32::MAX).1, [0, 0, 15]);
}

#[test]
fn edits_survive_restart_and_cache_eviction() {
    let path = test_dir();
    let mut world = World::with_capacity(42, path.clone(), 1).unwrap();
    let original = world.get_block(-1, 100, 16).unwrap();
    let replacement = if original == STONE { AIR } else { STONE };
    let (key, version) = world.edit(-1, 100, 16, replacement).unwrap();
    assert_eq!(key, ChunkKey { x: -1, y: 6, z: 1 });
    assert_eq!(version, 1);
    let (adjacent_key, adjacent_version) = world.edit(0, 100, 16, replacement).unwrap();
    assert_eq!(adjacent_key, ChunkKey { x: 0, y: 6, z: 1 });
    assert_eq!(adjacent_version, 1);
    world.get_chunk(ChunkKey { x: 100, y: 0, z: 0 }).unwrap();
    assert_eq!(world.get_block(-1, 100, 16).unwrap(), replacement);
    drop(world);
    let mut reopened = World::new(42, path.clone()).unwrap();
    assert_eq!(reopened.get_block(-1, 100, 16).unwrap(), replacement);
    assert_eq!(reopened.get_block(0, 100, 16).unwrap(), replacement);
    assert_eq!(reopened.get_chunk(key).unwrap().version, version);
    assert_eq!(reopened.edit(-1, 100, 16, original).unwrap().1, 2);
    drop(reopened);
    let mut reopened = World::new(42, path.clone()).unwrap();
    assert_eq!(reopened.get_block(-1, 100, 16).unwrap(), original);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn corrupt_save_is_not_silently_discarded() {
    let path = test_dir();
    let key = ChunkKey { x: 0, y: 0, z: 0 };
    let mut world = World::new(1, path.clone()).unwrap();
    world.edit(0, 15, 0, AIR).unwrap();
    drop(world);
    let save_path = path.join("0_0_0.bged");
    fs::write(save_path, b"corrupt").unwrap();
    assert!(World::new(1, path.clone()).unwrap().get_chunk(key).is_err());
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn interrupted_temporary_save_does_not_replace_committed_edit() {
    let path = test_dir();
    let mut world = World::new(7, path.clone()).unwrap();
    let (key, version) = world.edit(1, 30, 1, STONE).unwrap();
    drop(world);
    fs::write(path.join(".0_1_0.999.999.tmp"), b"partial write").unwrap();
    let mut reopened = World::new(7, path.clone()).unwrap();
    assert_eq!(reopened.get_block(1, 30, 1).unwrap(), STONE);
    assert_eq!(reopened.get_chunk(key).unwrap().version, version);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn terrain_is_deterministic_and_continuous_across_chunk_faces() {
    let seed = 0xB10C_6100;
    let origin = ChunkKey { x: -1, y: 0, z: 0 };
    let east = ChunkKey { x: 0, y: 0, z: 0 };
    let above = ChunkKey { x: -1, y: 1, z: 0 };
    let chunk = generate_chunk(origin, seed);
    assert_eq!(chunk, generate_chunk(origin, seed));
    assert_ne!(chunk, generate_chunk(origin, seed + 1));
    let east_chunk = generate_chunk(east, seed);
    let above_chunk = generate_chunk(above, seed);
    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            assert_eq!(
                chunk.block([15, y, z]),
                Some(generated_block(-1, y as i64, z as i64, seed))
            );
            assert_eq!(
                east_chunk.block([0, y, z]),
                Some(generated_block(0, y as i64, z as i64, seed))
            );
        }
        for x in 0..CHUNK_SIZE {
            assert_eq!(
                chunk.block([x, 15, z]),
                Some(generated_block(x as i64 - 16, 15, z as i64, seed))
            );
            assert_eq!(
                above_chunk.block([x, 0, z]),
                Some(generated_block(x as i64 - 16, 16, z as i64, seed))
            );
        }
    }
    for z in -64..=64 {
        let left = terrain_column(-1, z, seed).height;
        let right = terrain_column(0, z, seed).height;
        assert!((left - right).abs() <= 3, "height jumps at x chunk seam");
        let north = terrain_column(z, 15, seed).height;
        let south = terrain_column(z, 16, seed).height;
        assert!((north - south).abs() <= 3, "height jumps at z chunk seam");
    }
}

#[test]
fn terrain_has_broad_variation_and_caves() {
    let seed = 0xB10C_6100;
    let mut min_height = i64::MAX;
    let mut max_height = i64::MIN;
    let mut rocky_columns = 0;
    for z in (-512..=512).step_by(16) {
        for x in (-512..=512).step_by(16) {
            let column = terrain_column(x, z, seed);
            min_height = min_height.min(column.height);
            max_height = max_height.max(column.height);
            rocky_columns += usize::from(column.rocky);
            assert!(column.height <= i64::from(MAX_TERRAIN_HEIGHT));
        }
    }
    assert!(
        max_height - min_height >= 15,
        "terrain should have hills and valleys"
    );
    assert!(rocky_columns > 0, "rocky uplands should occur");

    let mut cave_air = 0;
    let mut cave_entrances = 0;
    for z in (-64..=64).step_by(4) {
        for x in (-64..=64).step_by(4) {
            let column = terrain_column(x, z, seed);
            cave_entrances +=
                usize::from(generated_block_in_column(x, column.height, z, column, seed) == AIR);
            for y in 0..column.height - 3 {
                cave_air += usize::from(generated_block_in_column(x, y, z, column, seed) == AIR);
            }
        }
    }
    assert!(cave_air > 0, "underground caves should occur");
    assert!(cave_entrances > 0, "some caves should open at the surface");
}

#[test]
fn biomes_cover_distinct_surfaces_across_an_endless_world() {
    let seed = 0xB10C_6100;
    let mut seen = [false; 5];
    let mut samples = [None; 5];
    for z in (-1024..=1024).step_by(32) {
        for x in (-1024..=1024).step_by(32) {
            let column = terrain_column(x, z, seed);
            let index = match column.biome {
                Biome::Plains => 0,
                Biome::Forest => 1,
                Biome::Desert => 2,
                Biome::Tundra => 3,
                Biome::Highland => 4,
            };
            seen[index] = true;
            samples[index].get_or_insert((x, z));
            assert!(column.height <= i64::from(MAX_TERRAIN_HEIGHT));
        }
    }
    assert!(seen.into_iter().all(|present| present));
    eprintln!("biome preview coordinates: {samples:?}");
    let distant = ChunkKey {
        x: 100_000,
        y: 1,
        z: -100_000,
    };
    assert_eq!(generate_chunk(distant, seed), generate_chunk(distant, seed));
}

#[test]
fn collapsed_surface_obeys_constraints_and_matches_region_edges() {
    let seed = 0xB10C_6100;
    let left = collapse_surface((0, 0), seed);
    let right = collapse_surface((1, 0), seed);
    assert_eq!(left, collapse_surface((0, 0), seed));
    assert!(
        left.iter().any(|tile| *tile >= 2),
        "collapse should make patches"
    );
    for field in [left, right] {
        for z in 0..8 {
            for x in 0..8 {
                let tile = field[x + z * 8];
                assert!(tile < 4);
                if x < 7 {
                    assert_ne!(
                        SURFACE_NEIGHBORS[tile as usize] & (1 << field[x + 1 + z * 8]),
                        0
                    );
                }
                if z < 7 {
                    assert_ne!(
                        SURFACE_NEIGHBORS[tile as usize] & (1 << field[x + (z + 1) * 8]),
                        0
                    );
                }
            }
        }
    }
    for z in 0..8 {
        assert_eq!(left[7 + z * 8], 0);
        assert_eq!(right[z * 8], 0);
    }
    for region_z in -4..4 {
        for region_x in -4..4 {
            let field = collapse_surface((region_x, region_z), seed);
            assert!(
                field.iter().any(|tile| *tile != 0),
                "collapse fell back to empty at region ({region_x}, {region_z})"
            );
        }
    }
}

#[test]
fn world_bottom_is_solid_and_cannot_be_edited() {
    let seed = 17;
    for (x, z) in [(0, 0), (123_456, -654_321)] {
        assert_eq!(generated_block(x, i64::from(BEDROCK_Y), z, seed), STONE);
        assert_eq!(
            generated_block(x, i64::from(BEDROCK_Y) - 1000, z, seed),
            STONE
        );
    }
    let path = test_dir();
    let mut world = World::new(seed, path.clone()).unwrap();
    assert_eq!(world.get_block(0, BEDROCK_Y, 0).unwrap(), STONE);
    assert_eq!(
        world.edit(0, BEDROCK_Y, 0, AIR).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn glowstone_edit_survives_restart() {
    let path = test_dir();
    let mut world = World::new(19, path.clone()).unwrap();
    world.edit(2, 35, 3, GLOWSTONE).unwrap();
    drop(world);
    let mut reopened = World::new(19, path.clone()).unwrap();
    assert_eq!(reopened.get_block(2, 35, 3).unwrap(), GLOWSTONE);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn old_generator_world_is_rejected_without_changing_its_metadata() {
    let path = test_dir();
    let mut metadata = Vec::from(*b"BGWD");
    metadata.extend_from_slice(&2u16.to_le_bytes());
    metadata.extend_from_slice(&17u64.to_le_bytes());
    let metadata_path = path.join("world.meta");
    std::fs::write(&metadata_path, &metadata).unwrap();
    assert_eq!(
        World::new(17, path.clone()).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(std::fs::read(metadata_path).unwrap(), metadata);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn legacy_edits_are_rejected_before_world_opens() {
    let path = test_dir();
    fs::write(path.join("0_0_0.bged"), b"legacy").unwrap();
    assert!(World::new(1, path.clone()).is_err());
    fs::remove_dir_all(path).unwrap();
}

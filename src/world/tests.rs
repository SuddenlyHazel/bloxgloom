use super::cache::ChunkCache;
use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
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
fn resident_arc_view_pins_a_version_without_copying_voxels() {
    let path = test_dir();
    let mut world = World::with_capacity(42, path.clone(), 1).unwrap();
    let (key, local) = world_to_chunk(0, 110, 0);
    world.get_chunk(key).unwrap();
    let old = world.cached_arc_chunk(key).unwrap();
    let another_reader = world.cached_arc_chunk(key).unwrap();
    assert!(Arc::ptr_eq(&old, &another_reader));
    let original = old.block(local).unwrap();
    let replacement = if original == STONE { AIR } else { STONE };
    let prepared = world.prepare_edit(0, 110, 0, replacement).unwrap();
    world.apply_prepared_edit(prepared).unwrap();
    let current = world.cached_arc_chunk(key).unwrap();
    assert!(!Arc::ptr_eq(&old, &current));
    assert_eq!(old.version, 0);
    assert_eq!(old.block(local), Some(original));
    assert_eq!(current.version, 1);
    assert_eq!(current.block(local), Some(replacement));
    world.get_chunk(ChunkKey { x: 30, y: 0, z: 30 }).unwrap();
    assert!(world.cached_arc_chunk(key).is_none());
    assert_eq!(old.block(local), Some(original));
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn worker_prepared_sparse_edit_matches_and_cannot_overwrite_a_newer_commit() {
    let path = test_dir();
    let mut world = World::new(42, path.clone()).unwrap();
    let (x, y, z) = (-1, 100, 16);
    let (key, local) = world_to_chunk(x, y, z);
    let original = world.get_block(x, y, z).unwrap();
    let replacement = if original == STONE { AIR } else { STONE };
    let basis = world.cached_edit_basis(key).unwrap();
    assert_eq!(basis.chunk().key, key);
    assert!(basis.catalog().state(replacement).is_some());
    let cell = Chunk::index(local).unwrap() as u16;
    let worker = thread::spawn(move || basis.prepare_sparse(&[(cell, replacement)]).unwrap());
    let main_prepared = world.prepare_edit(x, y, z, replacement).unwrap();
    let worker_prepared = worker.join().unwrap();
    assert_eq!(
        worker_prepared.before_snapshot,
        main_prepared.before_snapshot
    );
    assert_eq!(worker_prepared.after_snapshot, main_prepared.after_snapshot);
    assert_eq!(worker_prepared.new_version, main_prepared.new_version);
    world.apply_prepared_edit(main_prepared).unwrap();
    assert_eq!(
        world
            .apply_prepared_edit(worker_prepared)
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn chunk_cache_evicts_the_least_recently_used_resident() {
    let path = test_dir();
    let mut world = World::with_capacity(42, path.clone(), 2).unwrap();
    let first = ChunkKey { x: 0, y: 0, z: 0 };
    let second = ChunkKey { x: 1, y: 0, z: 0 };
    let third = ChunkKey { x: 2, y: 0, z: 0 };

    world.get_chunk(first).unwrap();
    world.get_chunk(second).unwrap();
    // A worker view is a real resident access and should refresh its recency.
    world.cached_arc_chunk(first).unwrap();
    world.get_chunk(third).unwrap();

    assert_eq!(world.resident_chunk_count(), 2);
    assert!(world.cached_version(first).is_some());
    assert!(world.cached_version(second).is_none());
    assert!(world.cached_version(third).is_some());
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn chunk_cache_unlinks_and_reinserts_entries_at_each_lru_position() {
    let mut cache = ChunkCache::new(4);
    let keys = [
        ChunkKey { x: 0, y: 0, z: 0 },
        ChunkKey { x: 1, y: 0, z: 0 },
        ChunkKey { x: 2, y: 0, z: 0 },
        ChunkKey { x: 3, y: 0, z: 0 },
        ChunkKey { x: 4, y: 0, z: 0 },
    ];
    let insert = |cache: &mut ChunkCache, key| {
        cache.insert(
            key,
            Arc::new(Chunk {
                key,
                version: 0,
                blocks: PalettedBlocks::uniform(AIR),
            }),
            BTreeMap::new(),
        );
    };

    for &key in &keys[..4] {
        insert(&mut cache, key);
    }

    // Remove and recycle a middle, oldest, and newest node. Each reinsertion
    // appends at the newest end, and the final admission must evict the true
    // oldest key rather than a detached or stale link.
    cache.remove(&keys[1]);
    insert(&mut cache, keys[1]);
    cache.remove(&keys[0]);
    insert(&mut cache, keys[0]);
    cache.remove(&keys[1]);
    insert(&mut cache, keys[1]);
    cache.remove(&keys[1]);
    insert(&mut cache, keys[1]);
    insert(&mut cache, keys[4]);

    assert!(!cache.contains_key(&keys[2]));
    assert!(cache.contains_key(&keys[0]));
    assert!(cache.contains_key(&keys[1]));
    assert!(cache.contains_key(&keys[3]));
    assert!(cache.contains_key(&keys[4]));
    assert_eq!(cache.len(), 4);
}

#[test]
fn subscribed_chunks_survive_cache_pressure_until_last_client_unpins() {
    let mut cache = ChunkCache::new(2);
    let keys = [
        ChunkKey { x: 0, y: 0, z: 0 },
        ChunkKey { x: 1, y: 0, z: 0 },
        ChunkKey { x: 2, y: 0, z: 0 },
    ];
    let insert = |cache: &mut ChunkCache, key| {
        cache.insert(
            key,
            Arc::new(Chunk {
                key,
                version: 0,
                blocks: PalettedBlocks::uniform(AIR),
            }),
            BTreeMap::new(),
        )
    };
    assert!(insert(&mut cache, keys[0]));
    assert!(insert(&mut cache, keys[1]));
    assert!(cache.pin(keys[0]));
    assert!(cache.pin(keys[0]));
    assert!(cache.pin(keys[1]));
    assert_eq!(cache.pinned_len(), 2);
    assert!(!cache.can_admit());
    assert!(!insert(&mut cache, keys[2]));
    assert!(cache.contains_key(&keys[0]));
    assert!(cache.contains_key(&keys[1]));
    assert!(cache.unpin(keys[0]));
    assert!(!cache.can_admit());
    assert!(cache.unpin(keys[0]));
    assert!(cache.can_admit());
    assert!(insert(&mut cache, keys[2]));
    assert!(!cache.contains_key(&keys[0]));
    assert!(cache.contains_key(&keys[1]));
    assert!(cache.contains_key(&keys[2]));
    assert_eq!(cache.pinned_len(), 1);
}

#[test]
fn edited_chunk_can_be_evicted_and_reloaded_from_its_pending_snapshot() {
    let path = test_dir();
    let mut world = World::with_capacity(42, path.clone(), 2).unwrap();
    let edited = ChunkKey { x: 0, y: 7, z: 0 };
    let second = ChunkKey { x: 1, y: 7, z: 0 };
    let third = ChunkKey { x: 2, y: 7, z: 0 };
    let fourth = ChunkKey { x: 3, y: 7, z: 0 };
    let original = world.get_block(0, 113, 0).unwrap();
    world.get_block(16, 113, 0).unwrap();
    let replacement = if original == STONE { AIR } else { STONE };
    let prepared = world.prepare_edit(0, 113, 0, replacement).unwrap();
    let snapshot = prepared.after_snapshot.clone();
    world.apply_prepared_edit(prepared).unwrap();

    // Applying an edit refreshes the edited chunk, so inserting a third chunk
    // evicts the untouched second one first.
    world.get_chunk(third).unwrap();
    assert!(world.cached_version(second).is_none());
    assert_eq!(world.cached_version(edited), Some(1));

    // A further insertion evicts the now-oldest edited chunk. Its WAL snapshot
    // remains authoritative and is used to reconstruct the chunk on demand.
    world.get_chunk(fourth).unwrap();
    assert!(world.cached_version(edited).is_none());
    assert_eq!(world.read_chunk_snapshot(edited).unwrap(), Some(snapshot));
    assert_eq!(world.get_block(0, 113, 0).unwrap(), replacement);
    assert_eq!(world.cached_version(edited), Some(1));
    drop(world);
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
fn wal_snapshot_remains_authoritative_after_cache_eviction() {
    let path = test_dir();
    let key = ChunkKey { x: -1, y: 7, z: 2 };
    let mut world = World::with_capacity(19, path.clone(), 1).unwrap();
    let x = -16;
    let y = 113;
    let z = 32;
    let original = world.get_block(x, y, z).unwrap();
    assert_eq!(world.cached_len(), 1);
    assert_eq!(world.cached_version(key), Some(0));
    assert_eq!(world.cached_block(x, y, z), Some(original));
    assert_eq!(world.cached_block(x + CHUNK_SIZE as i32, y, z), None);
    let replacement = if original == STONE { AIR } else { STONE };
    let prepared = world.prepare_edit(x, y, z, replacement).unwrap();
    let after_snapshot = prepared.after_snapshot.clone();
    world.apply_prepared_edit(prepared).unwrap();
    assert_eq!(world.cached_version(key), Some(1));
    assert_eq!(world.cached_block(x, y, z), Some(replacement));

    assert_eq!(world.storage.read_snapshot(key).unwrap(), None);
    assert_eq!(
        world.read_chunk_snapshot(key).unwrap(),
        Some(after_snapshot.clone())
    );
    world.get_chunk(ChunkKey { x: 20, y: 0, z: 20 }).unwrap();
    assert_eq!(world.cached_len(), 1);
    assert_eq!(world.cached_version(key), None);
    assert_eq!(world.cached_block(x, y, z), None);
    let (epoch, pending) = world.begin_chunk_load(key).unwrap();
    let pending = pending.expect("fresh loads should capture the uncheckpointed WAL value");
    assert_eq!(pending, after_snapshot);
    let loaded = world.load_chunk_snapshot_uncached(key, &pending).unwrap();
    assert!(world.install_loaded_if_absent(loaded, epoch).unwrap());
    assert_eq!(world.get_block(x, y, z).unwrap(), replacement);
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn prepared_edit_is_rejected_after_newer_edit_and_cache_eviction() {
    let path = test_dir();
    let mut world = World::with_capacity(23, path.clone(), 1).unwrap();
    let x = 0;
    let y = 113;
    let z = 0;
    let original = world.get_block(x, y, z).unwrap();
    let stale_value = if original == STONE { DIRT } else { STONE };
    let stale = world.prepare_edit(x, y, z, stale_value).unwrap();
    let newer_value = [AIR, DIRT, STONE]
        .into_iter()
        .find(|&block| block != original && block != stale_value)
        .unwrap();
    world.edit(x, y, z, newer_value).unwrap();
    world.get_chunk(ChunkKey { x: -30, y: 0, z: 8 }).unwrap();

    let error = world.apply_prepared_edit(stale).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(world.get_block(x, y, z).unwrap(), newer_value);
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn prepared_edit_survives_stale_loader_epoch_retirement() {
    let path = test_dir();
    let mut world = World::with_capacity(23, path.clone(), 1).unwrap();
    let (x, y, z) = (0, 113, 0);
    let key = world_to_chunk(x, y, z).0;
    let original = world.get_block(x, y, z).unwrap();
    let first = [AIR, DIRT, STONE]
        .into_iter()
        .find(|&block| block != original)
        .unwrap();
    let second = [AIR, DIRT, STONE]
        .into_iter()
        .find(|&block| block != original && block != first)
        .unwrap();
    let prepared_first = world.prepare_edit(x, y, z, first).unwrap();

    world.get_chunk(ChunkKey { x: 20, y: 0, z: 20 }).unwrap();
    let old_loaded = world.load_chunk_uncached(key).unwrap();
    let (old_epoch, _) = world.begin_chunk_load(key).unwrap();
    world.apply_prepared_edit(prepared_first).unwrap();
    let prepared_second = world.prepare_edit(x, y, z, second).unwrap();
    assert!(
        !world
            .install_loaded_if_absent(old_loaded, old_epoch)
            .unwrap()
    );
    assert_eq!(world.edit_epoch(key), 0);

    world.apply_prepared_edit(prepared_second).unwrap();
    assert_eq!(world.cached_block(x, y, z), Some(second));
    assert_eq!(world.cached_version(key), Some(2));
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn pre_edit_loader_result_is_rejected_after_edit_and_cache_eviction() {
    let path = test_dir();
    let mut world = World::with_capacity(29, path.clone(), 1).unwrap();
    let x = -32;
    let y = 113;
    let z = 48;
    let original = world.get_block(x, y, z).unwrap();
    let old_result = world
        .load_chunk_uncached(ChunkKey { x: -2, y: 7, z: 3 })
        .unwrap();
    let (old_epoch, _) = world
        .begin_chunk_load(ChunkKey { x: -2, y: 7, z: 3 })
        .unwrap();
    let replacement = if original == STONE { AIR } else { STONE };
    world.edit(x, y, z, replacement).unwrap();
    world
        .get_chunk(ChunkKey {
            x: 40,
            y: 0,
            z: -40,
        })
        .unwrap();

    assert!(
        !world
            .install_loaded_if_absent(old_result, old_epoch)
            .unwrap()
    );
    assert_eq!(world.get_block(x, y, z).unwrap(), replacement);
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn restore_snapshot_does_not_replace_a_corrupt_existing_snapshot() {
    let path = test_dir();
    let key = ChunkKey { x: 3, y: 7, z: -2 };
    let mut world = World::new(31, path.clone()).unwrap();
    let original = world.get_block(48, 113, -32).unwrap();
    let replacement = if original == STONE { AIR } else { STONE };
    let committed = world
        .prepare_edit(48, 113, -32, replacement)
        .unwrap()
        .after_snapshot;
    let save_path = path.join("3_7_-2.bged");
    fs::write(&save_path, b"corrupt prior snapshot").unwrap();

    let error = world.restore_snapshot(key, &committed).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(fs::read(save_path).unwrap(), b"corrupt prior snapshot");
    drop(world);
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
fn broadleaf_crowns_cross_chunk_seams_and_match_edit_baseline() {
    let seed = 0xB10C_6100;
    let tree = (-20..=20)
        .flat_map(|z| (-20..=20).map(move |x| (x, z)))
        .filter_map(|(x, z)| tree_anchor(x, z, seed))
        .find(|tree| tree.x.rem_euclid(16) >= 13 || tree.z.rem_euclid(16) >= 13)
        .expect("a tree crown should span a chunk seam");
    let mut chunks = HashMap::new();
    let mut leaves = 0;
    let mut wood = 0;
    for y in tree.ground_y + 1..=tree.trunk_top + 2 {
        for z in tree.z - TREE_RADIUS..=tree.z + TREE_RADIUS {
            for x in tree.x - TREE_RADIUS..=tree.x + TREE_RADIUS {
                let (key, local) = world_to_chunk(x as i32, y as i32, z as i32);
                let chunk = chunks
                    .entry(key)
                    .or_insert_with(|| generate_chunk(key, seed));
                let block = chunk.block(local).unwrap();
                assert_eq!(block, generated_block(x, y, z, seed), "at ({x}, {y}, {z})");
                leaves += usize::from(block == LEAVES);
                wood += usize::from(block == WOOD);
            }
        }
    }
    assert!(leaves > 20 && wood >= 5);

    let path = test_dir();
    let mut world = World::with_capacity(seed, path.clone(), 1).unwrap();
    let x = tree.x as i32;
    let y = (tree.trunk_top + 1) as i32;
    let z = tree.z as i32;
    let original = world.get_block(x, y, z).unwrap();
    assert_eq!(original, LEAVES);
    world.edit(x, y, z, AIR).unwrap();
    world
        .get_chunk(ChunkKey {
            x: 100,
            y: 0,
            z: 100,
        })
        .unwrap();
    assert_eq!(world.get_block(x, y, z).unwrap(), AIR);
    drop(world);
    let mut reopened = World::new(seed, path.clone()).unwrap();
    assert_eq!(reopened.get_block(x, y, z).unwrap(), AIR);
    reopened.edit(x, y, z, LEAVES).unwrap();
    assert_eq!(reopened.get_block(x, y, z).unwrap(), LEAVES);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn biome_plants_are_reproducible_and_non_solid() {
    assert!(!is_opaque(LEAVES));
    assert!(is_solid(LEAVES));
    for plant in [RED_FLOWER, YELLOW_FLOWER, BLUE_FLOWER, FERN, TALL_GRASS] {
        assert!(is_plant(plant));
        assert!(is_cutout(plant));
        assert!(!is_solid(plant));
        assert!(!is_opaque(plant));
        assert!(is_replaceable(plant));
    }
    let seed = 0xB10C_6100;
    let mut seen = [false; 5];
    for z in (-256..=256).step_by(4) {
        for x in (-256..=256).step_by(4) {
            let column = terrain_column(x, z, seed);
            let y = column.height + 1;
            let soil = generated_block_in_column(x, column.height, z, column, seed);
            let candidate = ground_plant(x, z, seed, column.biome, soil);
            if candidate == AIR || seen[(candidate.0 - RED_FLOWER.0) as usize] {
                continue;
            }
            let plant = generated_block(x, y, z, seed);
            if is_plant(plant) {
                let (key, local) = world_to_chunk(x as i32, y as i32, z as i32);
                assert_eq!(generate_chunk(key, seed).block(local), Some(plant));
                seen[(plant.0 - RED_FLOWER.0) as usize] = true;
                assert!(supports_plant(soil));
                assert!(matches!(column.biome, Biome::Plains | Biome::Forest));
            }
        }
        if seen.iter().all(|&present| present) {
            break;
        }
    }
    assert!(seen.into_iter().all(|present| present));
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
fn log_axis_states_survive_negative_and_positive_chunk_seams() {
    let path = test_dir();
    let mut world = World::new(61, path.clone()).unwrap();
    for (x, z, state) in [
        (-1, 0, WOOD_X),
        (0, 0, WOOD),
        (15, -1, WOOD_Z),
        (16, -1, WOOD_X),
    ] {
        world.edit(x, 100, z, state).unwrap();
    }
    drop(world);
    let mut reopened = World::new(61, path.clone()).unwrap();
    for (x, z, state) in [
        (-1, 0, WOOD_X),
        (0, 0, WOOD),
        (15, -1, WOOD_Z),
        (16, -1, WOOD_X),
    ] {
        assert_eq!(reopened.get_block(x, 100, z).unwrap(), state);
    }
    fs::remove_dir_all(path).unwrap();
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

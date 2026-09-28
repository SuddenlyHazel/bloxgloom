use super::*;
use crate::world::{AIR, CHUNK_VOLUME, STONE, World};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

struct Pattern;
impl bloxgloom_host_api::generation::Contributor for Pattern {
    fn generate(&self, context: Context, output: &mut Output) -> Result<(), GenerationError> {
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let local = [x, y, z];
                    if context.random_at(context.world_position(local)?, 37) & 1 == 0 {
                        output.set(local, "bloxgloom:stone")?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn registrations() -> Vec<Registration> {
    vec![Registration {
        key: "sample:pattern".into(),
        revision: 1,
        contributor: Arc::new(Pattern),
    }]
}

fn open(path: &Path, contributors: Vec<Registration>) -> io::Result<World> {
    World::with_generation(
        73,
        path.to_owned(),
        1,
        Arc::new(Catalog::builtins()),
        contributors,
    )
}

#[test]
fn unknown_builtin_generation_id_is_reported_without_panicking() {
    assert!(matches!(
        builtin_state_key(crate::content::BlockStateId(u32::MAX)),
        Err(GenerationError::Contributor(_))
    ));
}

#[test]
fn builtin_contributor_preserves_terrain_vegetation_and_negative_chunk_seams() {
    let catalog = Catalog::builtins();
    let seed = 73;
    // Frozen fingerprints from the pre-contributor generator, including a
    // negative horizontal seam, a vertical seam, bedrock, plants and empty sky.
    for (key, fingerprint) in [
        (ChunkKey { x: -2, y: 1, z: -1 }, 0xbcdc125b0b118626),
        (ChunkKey { x: -1, y: 1, z: -1 }, 0x23ece1ce2000def6),
        (ChunkKey { x: -1, y: 2, z: -1 }, 0x9c1bda7f8c872325),
        (ChunkKey { x: 0, y: 1, z: -1 }, 0x7790562fc8fd1ca5),
        (ChunkKey { x: 0, y: 2, z: -1 }, 0x9c1bda7f8c872325),
        (ChunkKey { x: -1, y: 1, z: 0 }, 0x6d2df6130ea01718),
        (ChunkKey { x: 0, y: -5, z: 0 }, 0x82c546d079aba325),
        (ChunkKey { x: 0, y: -4, z: 0 }, 0x67b3a0af5e4df9b6),
        (ChunkKey { x: 0, y: 8, z: 0 }, 0x9c1bda7f8c872325),
    ] {
        let expected = super::super::terrain::generate_blocks(key, seed);
        let composed = compose(key, seed, &catalog, &[]).unwrap();
        assert_eq!(
            composed.blocks.iter().copied().collect::<Vec<_>>(),
            expected,
            "{key:?}"
        );
        assert_eq!(
            format!("{:?}", composed.blocks),
            format!("{:?}", Chunk::from_blocks(key, 0, expected).blocks),
            "palette representation at {key:?}"
        );
        assert_eq!(generate_chunk(key, seed), composed, "{key:?}");
        let hash = composed
            .blocks
            .iter()
            .fold(0xcbf29ce484222325u64, |hash, block| {
                block.0.to_le_bytes().iter().fold(hash, |hash, byte| {
                    (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
                })
            });
        assert_eq!(hash, fingerprint, "{key:?}");
        for z in 0..16 {
            for x in 0..16 {
                for y in 0..16 {
                    let local = [x, y, z];
                    let world = [
                        i64::from(key.x) * 16 + x as i64,
                        i64::from(key.y) * 16 + y as i64,
                        i64::from(key.z) * 16 + z as i64,
                    ];
                    assert_eq!(
                        composed.block(local),
                        Some(super::super::terrain::generated_block(
                            world[0], world[1], world[2], seed
                        )),
                        "{key:?} {local:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn builtin_contributor_preserves_tree_canopy_across_chunk_seam() {
    let seed = 0xB10C_6100;
    let tree = (-20..=20)
        .flat_map(|z| (-20..=20).map(move |x| (x, z)))
        .filter_map(|(x, z)| super::super::terrain::tree_anchor(x, z, seed))
        .find(|tree| tree.x.rem_euclid(16) >= 13 || tree.z.rem_euclid(16) >= 13)
        .unwrap();
    let (first, _) =
        super::super::world_to_chunk(tree.x as i32, tree.trunk_top as i32, tree.z as i32);
    let (neighbor, _) = if tree.x.rem_euclid(16) >= 13 {
        super::super::world_to_chunk((tree.x + 3) as i32, tree.trunk_top as i32, tree.z as i32)
    } else {
        super::super::world_to_chunk(tree.x as i32, tree.trunk_top as i32, (tree.z + 3) as i32)
    };
    assert_ne!(first, neighbor);
    let catalog = Catalog::builtins();
    for key in [first, neighbor] {
        let baseline = super::super::terrain::generate_blocks(key, seed);
        let generated = compose(key, seed, &catalog, &[]).unwrap();
        assert_eq!(
            generated.blocks.iter().copied().collect::<Vec<_>>(),
            baseline
        );
        assert!(
            generated
                .blocks
                .iter()
                .any(|&block| block == super::super::LEAVES)
        );
    }
}

#[test]
fn builtin_edit_baseline_uses_contributor_output_at_negative_seam() {
    let path = crate::world::tests::test_dir();
    let mut world = open(&path, Vec::new()).unwrap();
    let key = ChunkKey { x: -1, y: 1, z: -1 };
    let baseline = world.get_chunk(key).unwrap();
    let seam_cells = [
        crate::world::Chunk::index([15, 0, 15]).unwrap(),
        crate::world::Chunk::index([0, 15, 0]).unwrap(),
        crate::world::Chunk::index([15, 15, 0]).unwrap(),
    ];
    let changed = seam_cells.map(|index| {
        (
            index as u16,
            if baseline.blocks[index] == AIR {
                STONE
            } else {
                AIR
            },
        )
    });
    let prepared = world
        .cached_edit_basis(key)
        .unwrap()
        .prepare_sparse(&changed)
        .unwrap();
    world.apply_prepared_edit(prepared).unwrap();
    let reset = seam_cells.map(|index| (index as u16, baseline.blocks[index]));
    let prepared = world
        .cached_edit_basis(key)
        .unwrap()
        .prepare_sparse(&reset)
        .unwrap();
    assert!(
        world
            .storage
            .decode_snapshot(Some(&prepared.after_snapshot))
            .unwrap()
            .blocks
            .is_empty()
    );
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn authoritative_generation_edit_baselines_survive_cache_miss_and_restart() {
    let path = crate::world::tests::test_dir();
    let mut world = open(&path, registrations()).unwrap();
    for key in [
        ChunkKey { x: -1, y: 8, z: -1 },
        ChunkKey { x: 0, y: 8, z: 0 },
    ] {
        // A missing authoritative cache entry remains unavailable, not generated air.
        assert!(world.cached_edit_basis(key).is_none());
        let loader = world.loader_view();
        let (epoch, _) = world.begin_chunk_load(key).unwrap();
        let loaded = loader.load_chunk_uncached(key).unwrap();
        let baseline = loaded.chunk.clone();
        assert!(world.install_loaded_if_absent(loaded, epoch).unwrap());
        let edits = baseline
            .blocks
            .iter()
            .enumerate()
            .map(|(index, &block)| (index as u16, if block == AIR { STONE } else { AIR }))
            .collect::<Vec<_>>();
        let prepared = world
            .cached_edit_basis(key)
            .unwrap()
            .prepare_sparse(&edits)
            .unwrap();
        assert_eq!(
            world
                .storage
                .decode_snapshot(Some(&prepared.after_snapshot))
                .unwrap()
                .blocks
                .len(),
            CHUNK_VOLUME
        );
        let snapshot = prepared.after_snapshot.clone();
        world.apply_prepared_edit(prepared).unwrap();
        // Evict while the WAL-backed overlay has not yet been checkpointed.
        world.get_chunk(ChunkKey { x: 7, y: 8, z: 7 }).unwrap();
        let (epoch, pending) = world.begin_chunk_load(key).unwrap();
        let loaded = loader
            .load_chunk_snapshot_uncached(key, &pending.unwrap())
            .unwrap();
        assert!(world.install_loaded_if_absent(loaded, epoch).unwrap());
        world.restore_snapshot(key, &snapshot).unwrap();
        drop(loader);
        drop(world);
        world = open(&path, registrations()).unwrap();
        let changed = world.get_chunk(key).unwrap();
        for &(cell, block) in &edits {
            assert_eq!(changed.blocks[usize::from(cell)], block);
        }
        // Every per-cell edit comparison must agree with the full-chunk baseline.
        let reset = baseline
            .blocks
            .iter()
            .enumerate()
            .map(|(i, &b)| (i as u16, b))
            .collect::<Vec<_>>();
        let prepared = world
            .cached_edit_basis(key)
            .unwrap()
            .prepare_sparse(&reset)
            .unwrap();
        assert!(
            world
                .storage
                .decode_snapshot(Some(&prepared.after_snapshot))
                .unwrap()
                .blocks
                .is_empty()
        );
        let snapshot = prepared.after_snapshot.clone();
        world.apply_prepared_edit(prepared).unwrap();
        world.restore_snapshot(key, &snapshot).unwrap();
        drop(world);
        world = open(&path, registrations()).unwrap();
        assert_eq!(world.get_chunk(key).unwrap().blocks, baseline.blocks);
    }
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

fn files(path: &Path) -> BTreeMap<std::ffi::OsString, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), fs::read(entry.path()).unwrap())
        })
        .collect()
}

#[test]
fn generation_identity_mismatch_rejects_before_any_save_mutation() {
    let path = crate::world::tests::test_dir();
    let mut original = registrations();
    let mut second = original[0].clone();
    second.key = "sample:second".into();
    original.push(second);
    let mut world = open(&path, original.clone()).unwrap();
    let original_block = world.get_block(-1, 140, 0).unwrap();
    world
        .edit(-1, 140, 0, if original_block == AIR { STONE } else { AIR })
        .unwrap();
    drop(world);
    // Registration order isn't identity; lexical execution order is.
    original.reverse();
    drop(open(&path, original.clone()).unwrap());
    fs::remove_file(path.join(crate::storage::WORLD_LOCK)).unwrap();
    let before = files(&path);
    let mut changed_revision = original.clone();
    changed_revision[0].revision += 1;
    let mut changed_key = original.clone();
    changed_key[0].key = "sample:replacement".into();
    for entries in [Vec::new(), registrations(), changed_revision, changed_key] {
        assert_eq!(
            open(&path, entries).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(files(&path), before);
    }
    // Even without BGED files (e.g. a WAL-only world), missing metadata must not
    // let a builtin-only restart adopt the remaining save under a new identity.
    fs::remove_file(path.join("-1_8_0.bged")).unwrap();
    fs::remove_file(path.join("world.meta")).unwrap();
    let before = files(&path);
    assert_eq!(
        open(&path, Vec::new()).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(files(&path), before);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn generation_failure_never_installs_air_or_replaces_recovery_snapshot() {
    let path = crate::world::tests::test_dir();
    let mut world = open(
        &path,
        vec![Registration {
            key: "sample:invalid".into(),
            revision: 1,
            contributor: Arc::new(crate::world::tests::FixedGeneration("sample:missing")),
        }],
    )
    .unwrap();
    let key = ChunkKey { x: 0, y: 8, z: 0 };
    world
        .storage
        .save(
            key,
            &crate::storage::SavedEdits {
                version: 1,
                blocks: BTreeMap::from([(0, STONE)]),
            },
        )
        .unwrap();
    let replacement = world
        .storage
        .encode_snapshot(&crate::storage::SavedEdits {
            version: 2,
            blocks: BTreeMap::from([(0, AIR)]),
        })
        .unwrap()
        .unwrap();
    let before = files(&path);
    assert!(world.load_chunk_uncached(key).is_err());
    assert!(world.get_block(0, 128, 0).is_err());
    assert_eq!(world.cached_block(0, 128, 0), None);
    assert!(world.cached_edit_basis(key).is_none());
    assert!(world.restore_snapshot(key, &replacement).is_err());
    assert_eq!(files(&path), before);
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

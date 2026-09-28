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

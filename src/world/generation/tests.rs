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

struct SourcePattern([u8; 32]);
impl Contributor for SourcePattern {
    fn source_identity(&self) -> Option<[u8; 32]> {
        Some(self.0)
    }
    fn generate(&self, context: Context, output: &mut Output) -> Result<(), GenerationError> {
        Pattern.generate(context, output)
    }
}

#[test]
fn generation_source_change_rejects_same_revision_before_mutating_world() {
    let path = crate::world::tests::test_dir();
    let source = |digest| {
        vec![Registration {
            key: "sample:pattern".into(),
            revision: 1,
            contributor: Arc::new(SourcePattern([digest; 32])),
        }]
    };
    let mut world = open(&path, source(1)).unwrap();
    world.edit(0, 140, 0, STONE).unwrap();
    drop(world);
    drop(open(&path, source(1)).unwrap());
    fs::remove_file(path.join(crate::storage::WORLD_LOCK)).unwrap();
    let before = files(&path);
    for changed in [source(2), registrations()] {
        let error = open(&path, changed).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("sample:pattern"), "{error}");
        assert_eq!(files(&path), before);
    }
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn generation_source_identity_fits_full_contributor_and_key_limits() {
    let registrations = (0..256)
        .map(|i| Registration {
            key: format!("sample:{i:03}{}", "x".repeat(245)),
            revision: 1,
            contributor: Arc::new(SourcePattern([i as u8; 32])),
        })
        .collect();
    let generator = Generator::new(registrations).unwrap();
    assert_eq!(generator.identity().len(), MAX_GENERATION_IDENTITY_BYTES);
    assert_eq!(Generator::default().identity(), [0, 0]);
}

#[test]
fn builtin_samples_match_base_terrain_on_negative_and_vertical_seams() {
    let seed = 73;
    for x in [-17, -16, -1, 0, 15, 16] {
        for z in [-16, -1, 0, 15, 16] {
            let context = Context::with_samples(seed, [-1, 1, 0], &BUILTIN_SAMPLES);
            let column = super::super::terrain::terrain_column(x, z, seed);
            assert_eq!(context.builtin_terrain_height(x, z), Ok(column.height));
            for y in [-65, -64, -1, 0, 15, 16, 31, 32, 74, 75] {
                let expected = if y <= i64::from(super::super::BEDROCK_Y) {
                    STONE
                } else if y > i64::from(super::super::MAX_GENERATED_HEIGHT) {
                    AIR
                } else {
                    super::super::terrain::generated_block_in_column(x, y, z, column, seed)
                };
                assert_eq!(
                    context.builtin_base_block([x, y, z]),
                    Ok(builtin_state_key(expected).unwrap()),
                    "at {x}, {y}, {z}"
                );
            }
        }
    }
    let min = i64::from(i32::MIN) * 16;
    let max = i64::from(i32::MAX) * 16 + 15;
    let context = Context::with_samples(seed, [0; 3], &BUILTIN_SAMPLES);
    assert!(context.builtin_terrain_height(min, max).is_ok());
    assert_eq!(
        context.builtin_terrain_height(min - 1, 0),
        Err(bloxgloom_host_api::generation::SampleError::OutOfBounds)
    );
    assert_eq!(
        context.builtin_base_block([0, max + 1, 0]),
        Err(bloxgloom_host_api::generation::SampleError::OutOfBounds)
    );
}

/// A feature anchored at absolute x=0 deliberately extends into the negative
/// neighbor. Each destination chunk computes only its own cells from that anchor.
struct AcrossSeam;
impl Contributor for AcrossSeam {
    fn generate(&self, context: Context, output: &mut Output) -> Result<(), GenerationError> {
        assert!(context.builtin_terrain_height(0, 0).is_ok());
        assert_eq!(context.builtin_base_block([0, 128, 0]), Ok("bloxgloom:air"));
        for x in 0..16 {
            let [absolute_x, _, _] = context.world_position([x, 0, 0])?;
            if (-1..=0).contains(&absolute_x) {
                output.set([x, 0, 0], "bloxgloom:glowstone")?;
            }
        }
        Ok(())
    }
}

#[test]
fn absolute_anchor_feature_is_not_truncated_at_chunk_boundary() {
    let registration = Registration {
        key: "example:seam".into(),
        revision: 1,
        contributor: Arc::new(AcrossSeam),
    };
    let catalog = Catalog::builtins();
    let left = generate_chunk_with_contributors(
        ChunkKey { x: -1, y: 8, z: 0 },
        73,
        &catalog,
        std::slice::from_ref(&registration),
    )
    .unwrap();
    let right = generate_chunk_with_contributors(
        ChunkKey { x: 0, y: 8, z: 0 },
        73,
        &catalog,
        &[registration],
    )
    .unwrap();
    assert_eq!(left.block([15, 0, 0]), Some(super::super::GLOWSTONE));
    assert_eq!(right.block([0, 0, 0]), Some(super::super::GLOWSTONE));
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
    // Frozen fingerprints from generator version 6, including a
    // negative horizontal seam, a vertical seam, bedrock, plants and empty sky.
    for (key, fingerprint) in [
        (ChunkKey { x: -2, y: 1, z: -1 }, 0x2eeb922e1b1a3f7a),
        (ChunkKey { x: -1, y: 1, z: -1 }, 0x0d7122465dd1ed06),
        (ChunkKey { x: -1, y: 2, z: -1 }, 0x9c1bda7f8c872325),
        (ChunkKey { x: 0, y: 1, z: -1 }, 0xb62ff1cb98428748),
        (ChunkKey { x: 0, y: 2, z: -1 }, 0x9c1bda7f8c872325),
        (ChunkKey { x: -1, y: 1, z: 0 }, 0x178428e886829d2e),
        (ChunkKey { x: 0, y: -5, z: 0 }, 0x82c546d079aba325),
        (ChunkKey { x: 0, y: -4, z: 0 }, 0x440d30e377554c1c),
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
        .find(|tree| {
            tree.log == super::super::WOOD
                && (tree.x.rem_euclid(16) >= 13 || tree.z.rem_euclid(16) >= 13)
        })
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
        assert!(generated.blocks.iter().any(|&block| block == tree.leaves));
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

/// Deterministic accounting exercises admission without scheduler-dependent sleeps.
struct Charged(std::time::Duration);
impl Contributor for Charged {
    fn generate(&self, _: Context, output: &mut Output) -> Result<(), GenerationError> {
        output.set([0, 0, 0], "bloxgloom:stone")
    }

    fn generate_budgeted(
        &self,
        context: Context,
        output: &mut Output,
        _: std::time::Duration,
    ) -> Result<std::time::Duration, GenerationError> {
        self.generate(context, output)?;
        Ok(self.0)
    }
}

#[test]
fn scripted_generation_shares_one_chunk_allowance_and_rejects_whole_candidate() {
    let catalog = Catalog::builtins();
    let key = ChunkKey { x: 0, y: 8, z: 0 };
    let charged = ["budget:a", "budget:b"].map(|key| Registration {
        key: key.into(),
        revision: 1,
        contributor: Arc::new(Charged(std::time::Duration::from_millis(60))),
    });
    let error = generate_chunk_with_contributors(key, 73, &catalog, &charged).unwrap_err();
    assert!(matches!(error, GenerationError::Contributor(ref message)
        if message.contains("budget:b") && message.contains("chunk allowance")));
    // Rejection cannot poison a retry or publish a partial chunk.
    assert!(generate_chunk_with_contributors(key, 73, &catalog, &charged[..1]).is_ok());
    let exact = Registration {
        key: "budget:exact".into(),
        revision: 1,
        contributor: Arc::new(Charged(
            crate::server::script_capacity::GENERATION_SCRIPT_WALL_TIME,
        )),
    };
    // Trusted native work still runs after the scripted allowance is exhausted.
    assert!(
        generate_chunk_with_contributors(key, 73, &catalog, &[exact, registrations()[0].clone()])
            .is_ok()
    );
}

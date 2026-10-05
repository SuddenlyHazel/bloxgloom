use super::*;
mod persistent;
fn temporary() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "bloxgloom-lod-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
#[test]
fn worker_captures_uncheckpointed_high_structure_without_gameplay_cache_growth() {
    let root = temporary();
    let mut world = World::with_capacity(7, root.clone(), 4).unwrap();
    world.get_chunk(ChunkKey { x: 0, y: 10, z: 0 }).unwrap();
    let edit = world
        .prepare_edit(0, 160, 0, crate::world::GLOWSTONE)
        .unwrap();
    world.apply_prepared_edit(edit).unwrap();
    let key = TileKey {
        level: 0,
        x: 0,
        z: 0,
    };
    let overlays = world.lod_overlays(key.bounds().unwrap()).unwrap();
    let count = world.cached_len();
    let pins = world.pinned_chunk_count();
    let mut service = Service::new(&world, root.clone()).unwrap();
    service
        .jobs
        .as_ref()
        .unwrap()
        .try_send(worker::Job {
            key,
            revision: 1,
            overlays,
            resident: Vec::new(),
            children: None,
            requested_at: Instant::now(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();
    let completion = service
        .results
        .recv_timeout(Duration::from_secs(20))
        .unwrap();
    let tile = completion.tile.expect("bounded base tile generated");
    assert!(
        tile.columns[0]
            .spans
            .iter()
            .any(|v| v.bottom <= 160 && v.top > 160 && v.state == crate::world::GLOWSTONE)
    );
    assert_eq!(world.cached_len(), count);
    assert_eq!(world.pinned_chunk_count(), pins);
    // Same-session derived cache is reusable, but an edit advances its namespace.
    service
        .jobs
        .as_ref()
        .unwrap()
        .try_send(worker::Job {
            key,
            revision: 1,
            overlays: world.lod_overlays(key.bounds().unwrap()).unwrap(),
            resident: Vec::new(),
            children: None,
            requested_at: Instant::now(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();
    let warm = service
        .results
        .recv_timeout(Duration::from_secs(20))
        .unwrap();
    assert!(warm.cache_hit);
    assert_eq!(warm.tile.as_ref(), Some(&tile));
    eprintln!(
        "LOD cold={}ms warm={}ms bytes={}",
        completion.elapsed.as_millis(),
        warm.elapsed.as_millis(),
        crate::protocol::server_wire_len(&ServerMessage::LodTile {
            session: 1,
            request: 1,
            tile: tile.clone()
        })
    );
    // A plausible state-bit corruption must rebuild rather than become terrain.
    let cache_path = root.join("lod-cache").join("0_0_0.tile");
    let mut damaged = std::fs::read(&cache_path).unwrap();
    damaged[65] ^= 1;
    std::fs::write(&cache_path, damaged).unwrap();
    service
        .jobs
        .as_ref()
        .unwrap()
        .try_send(worker::Job {
            key,
            revision: 1,
            overlays: world.lod_overlays(key.bounds().unwrap()).unwrap(),
            resident: Vec::new(),
            children: None,
            requested_at: Instant::now(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();
    assert_eq!(
        service
            .results
            .recv_timeout(Duration::from_secs(20))
            .unwrap()
            .tile
            .as_ref(),
        Some(&tile)
    );
    let edit = world.prepare_edit(0, 160, 0, crate::world::AIR).unwrap();
    world.apply_prepared_edit(edit).unwrap();
    service.invalidate();
    assert_ne!(service.revision, completion.revision);
    service
        .jobs
        .as_ref()
        .unwrap()
        .try_send(worker::Job {
            key,
            revision: service.revision,
            overlays: world.lod_overlays(key.bounds().unwrap()).unwrap(),
            resident: Vec::new(),
            children: None,
            requested_at: Instant::now(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();
    let refreshed = service
        .results
        .recv_timeout(Duration::from_secs(20))
        .unwrap()
        .tile
        .unwrap();
    assert!(
        !refreshed.columns[0]
            .spans
            .iter()
            .any(|v| v.bottom <= 160 && v.top > 160)
    );
    drop(service);
    drop(world);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancelled_builds_retire_every_budget_slot_and_shutdown_with_full_completion_queue() {
    let root = temporary();
    let world = World::with_capacity(7, root.clone(), 4).unwrap();
    let mut service = Service::new(&world, root.clone()).unwrap();
    for x in 0..MAX_PENDING as i32 {
        let key = TileKey { level: 4, x, z: 0 };
        let cancelled = Arc::new(AtomicBool::new(false));
        service
            .jobs
            .as_ref()
            .unwrap()
            .try_send(worker::Job {
                key,
                revision: 1,
                overlays: vec![],
                resident: Vec::new(),
                children: None,
                requested_at: Instant::now(),
                cancelled: cancelled.clone(),
            })
            .unwrap();
        service.pending.insert(key, (1, cancelled));
    }
    service.invalidate();
    for _ in 0..MAX_PENDING {
        let result = service
            .results
            .recv_timeout(Duration::from_secs(20))
            .unwrap();
        service.pending.remove(&result.key);
        assert_ne!(result.revision, service.revision);
    }
    assert!(service.pending.is_empty());
    // Four completions fit even if the coordinator shuts down without polling.
    for x in 0..MAX_PENDING as i32 {
        let key = TileKey { level: 4, x, z: 0 };
        let cancelled = Arc::new(AtomicBool::new(false));
        service
            .jobs
            .as_ref()
            .unwrap()
            .try_send(worker::Job {
                key,
                revision: service.revision,
                overlays: vec![],
                resident: Vec::new(),
                children: None,
                requested_at: Instant::now(),
                cancelled: cancelled.clone(),
            })
            .unwrap();
        service.pending.insert(key, (service.revision, cancelled));
    }
    let started = Instant::now();
    drop(service);
    assert!(started.elapsed() < Duration::from_secs(20));
    drop(world);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unrelated_commit_keeps_inflight_tile_while_related_commit_rejects_old_result() {
    let root = temporary();
    let mut state = crate::server::server_state(7, root.clone()).unwrap();
    let near = TileKey {
        level: 4,
        x: 0,
        z: 0,
    };
    let distant = TileKey {
        level: 4,
        x: 2,
        z: 0,
    };
    for key in [distant, near] {
        let cancelled = Arc::new(AtomicBool::new(false));
        state
            .lod
            .jobs
            .as_ref()
            .unwrap()
            .try_send(worker::Job {
                key,
                revision: 1,
                overlays: vec![],
                resident: Vec::new(),
                children: None,
                requested_at: Instant::now(),
                cancelled: cancelled.clone(),
            })
            .unwrap();
        state.lod.pending.insert(key, (1, cancelled));
    }
    // A real newer committed overlay exists before these completions install.
    state
        .world
        .get_chunk(ChunkKey { x: 0, y: 6, z: 0 })
        .unwrap();
    let edit = state
        .world
        .prepare_edit(0, 100, 0, crate::world::GLOWSTONE)
        .unwrap();
    state.world.apply_prepared_edit(edit).unwrap();
    invalidate(&mut state, [ChunkKey { x: 0, y: 6, z: 0 }]);
    assert_eq!(state.lod.revision, 2);
    assert!(!state.lod.pending[&distant].1.load(Ordering::Relaxed));
    assert!(state.lod.pending[&near].1.load(Ordering::Relaxed));
    for _ in 0..2 {
        let result = state
            .lod
            .results
            .recv_timeout(Duration::from_secs(20))
            .unwrap();
        if result.key == distant {
            assert!(result.tile.is_some());
            assert!(
                state.lod.finish(&result),
                "unrelated edit must not starve valid distant builds"
            );
        } else {
            assert!(
                !state.lod.finish(&result),
                "related old work must never install"
            );
        }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    state
        .lod
        .jobs
        .as_ref()
        .unwrap()
        .try_send(worker::Job {
            key: near,
            revision: 2,
            overlays: state.world.lod_overlays(near.bounds().unwrap()).unwrap(),
            resident: Vec::new(),
            children: None,
            requested_at: Instant::now(),
            cancelled: cancelled.clone(),
        })
        .unwrap();
    state.lod.pending.insert(near, (2, cancelled));
    let result = state
        .lod
        .results
        .recv_timeout(Duration::from_secs(20))
        .unwrap();
    assert!(state.lod.finish(&result));
    assert!(
        result.tile.unwrap().columns[0]
            .spans
            .iter()
            .any(|s| s.bottom <= 100 && s.top > 100 && s.state == crate::world::GLOWSTONE)
    );
    drop(state);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn revision_exhaustion_cancels_every_inflight_dependency() {
    let root = temporary();
    let world = World::with_capacity(7, root.clone(), 4).unwrap();
    let mut service = Service::new(&world, root.clone()).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    service.pending.insert(
        TileKey {
            level: 4,
            x: 0,
            z: 0,
        },
        (u64::MAX, cancelled.clone()),
    );
    service.revision = u64::MAX;
    assert!(!service.advance_revision());
    assert!(service.exhausted);
    assert!(cancelled.load(Ordering::Relaxed));
    drop(service);
    drop(world);
    std::fs::remove_dir_all(root).unwrap();
}

struct ObservedHighTerrain;
impl bloxgloom_host_api::generation::Contributor for ObservedHighTerrain {
    fn generate(
        &self,
        context: bloxgloom_host_api::generation::Context,
        output: &mut bloxgloom_host_api::generation::Output,
    ) -> Result<(), bloxgloom_host_api::generation::GenerationError> {
        if context.chunk == [0, 10, 0] {
            output.set([0, 0, 0], "bloxgloom:wood[axis=y]")?;
        }
        Ok(())
    }
}
fn observed_world(root: &std::path::Path, capacity: usize) -> World {
    World::with_generation(
        7,
        root.to_owned(),
        capacity,
        Arc::new(crate::content::Catalog::builtins()),
        vec![bloxgloom_host_api::generation::Registration {
            key: "lod:observed_high".into(),
            revision: 1,
            contributor: Arc::new(ObservedHighTerrain),
        }],
    )
    .unwrap()
}
#[test]
fn resident_high_contributor_coverage_survives_eviction_without_save_data_or_pins() {
    let root = temporary();
    let mut world = observed_world(&root, 1);
    let source = ChunkKey { x: 0, y: 10, z: 0 };
    world.get_chunk(source).unwrap();
    assert!(
        world
            .storage_handle()
            .read_snapshot(source)
            .unwrap()
            .is_none()
    );
    let key = TileKey {
        level: 1,
        x: 0,
        z: 0,
    };
    let resident = world.lod_resident(key.bounds().unwrap()).unwrap();
    assert_eq!(resident.len(), 1);
    assert_eq!(resident[0].version, 0);
    assert_eq!(world.pinned_chunk_count(), 0);
    // Immutable observed coverage survives normal gameplay LRU eviction.
    world
        .get_chunk(ChunkKey {
            x: 100,
            y: 10,
            z: 0,
        })
        .unwrap();
    assert!(world.cached_version(source).is_none());
    let count = world.cached_len();
    let service = Service::new(&world, root.clone()).unwrap();
    service
        .jobs
        .as_ref()
        .unwrap()
        .try_send(worker::Job {
            key,
            revision: 1,
            overlays: vec![],
            resident,
            children: None,
            requested_at: Instant::now(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();
    let tile = service
        .results
        .recv_timeout(Duration::from_secs(20))
        .unwrap()
        .tile
        .unwrap();
    assert!(tile.columns[0].known(160, 176));
    assert!(!tile.columns[0].known(176, 192));
    assert!(
        tile.columns[0]
            .spans
            .iter()
            .any(|s| s.bottom <= 160 && s.top > 160 && s.state == crate::world::WOOD)
    );
    assert_eq!(world.cached_len(), count);
    assert_eq!(world.pinned_chunk_count(), 0);
    drop(service);
    drop(world);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn resident_capture_budget_rejects_excessive_observed_columns_and_ignores_builtin_high_air() {
    let root = temporary();
    let mut world = observed_world(&root, 2049);
    let key = TileKey {
        level: 1,
        x: 0,
        z: 0,
    };
    for y in 11..2060 {
        world.get_chunk(ChunkKey { x: 0, y, z: 0 }).unwrap();
    }
    assert!(world.lod_resident(key.bounds().unwrap()).is_err());
    assert_eq!(world.pinned_chunk_count(), 0);
    drop(world);
    std::fs::remove_dir_all(&root).unwrap();
    let root = temporary();
    let mut builtin = World::with_capacity(7, root.clone(), 1).unwrap();
    builtin.get_chunk(ChunkKey { x: 0, y: 20, z: 0 }).unwrap();
    assert!(
        builtin
            .lod_resident(key.bounds().unwrap())
            .unwrap()
            .is_empty()
    );
    drop(builtin);
    std::fs::remove_dir_all(root).unwrap();
}

impl bloxgloom_host_api::Extension for ObservedHighTerrain {
    fn register(
        &self,
        host: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), bloxgloom_host_api::RegistrationError> {
        host.generation_contributor(bloxgloom_host_api::generation::Registration {
            key: "lod:observed_high".into(),
            revision: 1,
            contributor: Arc::new(ObservedHighTerrain),
        })
    }
}
#[test]
fn new_authoritative_high_load_invalidates_cached_partial_contributor_summary() {
    let root = temporary();
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&ObservedHighTerrain)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(7, root.clone(), 8, startup).unwrap();
    let key = TileKey {
        level: 1,
        x: 0,
        z: 0,
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    state.lod.pending.insert(key, (1, cancelled.clone()));
    state
        .lod
        .jobs
        .as_ref()
        .unwrap()
        .try_send(worker::Job {
            key,
            revision: 1,
            overlays: vec![],
            resident: vec![],
            children: None,
            requested_at: Instant::now(),
            cancelled,
        })
        .unwrap();
    let result = state
        .lod
        .results
        .recv_timeout(Duration::from_secs(20))
        .unwrap();
    assert!(state.lod.finish(&result));
    let tile = result.tile.unwrap();
    assert!(!tile.columns[0].known(160, 176));
    state.lod.cache.insert(key, tile);
    let source = ChunkKey { x: 0, y: 10, z: 0 };
    assert!(crate::server::streaming::request_chunk(&mut state, source).unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    while state.world.cached_version(source).is_none() {
        crate::server::streaming::poll_chunk_loads(&mut state).unwrap();
        assert!(
            Instant::now() < deadline,
            "observed high loader completion timeout"
        );
        std::thread::yield_now();
    }
    assert!(state.lod.revision > 1);
    assert!(!state.lod.cache.contains_key(&key));
    assert_eq!(
        state.world.cached_block(0, 160, 0),
        Some(crate::world::WOOD)
    );
    drop(state);
    std::fs::remove_dir_all(root).unwrap();
}

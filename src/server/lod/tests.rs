use super::*;
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
            children: None,
            requested_at: Instant::now(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();
    let warm = service
        .results
        .recv_timeout(Duration::from_secs(20))
        .unwrap();
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
    let cache_path = root.join("lod-cache").join("1_0_0_0.tile");
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

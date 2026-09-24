use super::*;
use crate::world::{AIR, DIRT, STONE};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn test_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-loader-{}-{stamp}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn requests_are_deduplicated_and_negative_chunks_load_asynchronously() {
    let path = test_dir();
    let mut world = World::new(37, path.clone()).unwrap();
    let key = ChunkKey { x: -2, y: 8, z: -3 };
    let mut loader = ChunkLoader::new(37, path.clone(), 4).unwrap();
    let ticket = match loader.request(&mut world, key).unwrap() {
        RequestStatus::Enqueued(ticket) => ticket,
        RequestStatus::AlreadyPending(_) => panic!("first request cannot be a duplicate"),
    };
    assert_eq!(
        loader.request(&mut world, key).unwrap(),
        RequestStatus::AlreadyPending(ticket)
    );
    assert!(loader.is_pending(key));
    assert!(world.cached_chunk(key).is_none());

    let deadline = Instant::now() + Duration::from_secs(5);
    let result = loop {
        match loader.try_recv() {
            Ok(result) => break result,
            Err(TryRecvError::Empty) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(2));
            }
            other => panic!("chunk load did not complete: {other:?}"),
        }
    };
    assert_eq!(result.ticket, ticket);
    let loaded = result.result.unwrap();
    assert_eq!(loaded.chunk.key, key);
    assert!(
        world
            .install_loaded_if_absent(loaded, ticket.edit_epoch)
            .unwrap()
    );
    assert!(world.cached_chunk(key).is_some());
    assert!(!loader.is_pending(key));
    drop(loader);
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn accepted_work_budget_has_explicit_nonblocking_overflow() {
    let path = test_dir();
    let mut world = World::new(41, path.clone()).unwrap();
    let mut loader = ChunkLoader::new(41, path.clone(), 2).unwrap();
    for key in [
        ChunkKey {
            x: -10,
            y: 8,
            z: -10,
        },
        ChunkKey { x: 10, y: 8, z: 10 },
    ] {
        assert!(matches!(
            loader.request(&mut world, key),
            Ok(RequestStatus::Enqueued(_))
        ));
    }
    let full_key = ChunkKey {
        x: 1_000,
        y: 8,
        z: -1_000,
    };
    assert!(matches!(
        loader.request(&mut world, full_key),
        Err(RequestError::QueueFull)
    ));
    assert!(!loader.is_pending(full_key));
    assert_eq!(loader.capacity(), 2);
    assert_eq!(loader.outstanding(), 2);

    drop(loader);
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn worker_load_uses_uncheckpointed_authoritative_snapshot_after_eviction() {
    let path = test_dir();
    let key = ChunkKey { x: -1, y: 7, z: 2 };
    let (x, y, z) = (-16, 113, 32);
    let mut world = World::with_capacity(43, path.clone(), 1).unwrap();
    let original = world.get_block(x, y, z).unwrap();
    let replacement = [AIR, DIRT, STONE]
        .into_iter()
        .find(|&block| block != original)
        .unwrap();
    let prepared = world.prepare_edit(x, y, z, replacement).unwrap();
    let expected_version = prepared.new_version;
    world.apply_prepared_edit(prepared).unwrap();
    assert_eq!(world.storage_handle().read_snapshot(key).unwrap(), None);
    world.get_chunk(ChunkKey { x: 40, y: 0, z: 40 }).unwrap();

    let mut loader = ChunkLoader::new(43, path.clone(), 2).unwrap();
    let ticket = match loader.request(&mut world, key).unwrap() {
        RequestStatus::Enqueued(ticket) => ticket,
        RequestStatus::AlreadyPending(_) => panic!("request should be new"),
    };
    let result = receive_before(&mut loader, Instant::now() + Duration::from_secs(5));
    assert_eq!(result.ticket, ticket);
    let loaded = result.result.unwrap();
    assert_eq!(loaded.chunk.version, expected_version);
    assert!(
        world
            .install_loaded_if_absent(loaded, ticket.edit_epoch)
            .unwrap()
    );
    assert_eq!(world.get_block(x, y, z).unwrap(), replacement);
    assert_eq!(world.get_chunk(key).unwrap().version, expected_version);
    drop(loader);
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn pre_edit_worker_result_is_rejected_after_uncheckpointed_edit() {
    let path = test_dir();
    let key = ChunkKey { x: 2, y: 7, z: -3 };
    let (x, y, z) = (32, 113, -48);
    let mut world = World::with_capacity(47, path.clone(), 1).unwrap();
    let original = world.get_block(x, y, z).unwrap();
    let mut loader = ChunkLoader::new(47, path.clone(), 2).unwrap();
    let ticket = match loader.request(&mut world, key).unwrap() {
        RequestStatus::Enqueued(ticket) => ticket,
        RequestStatus::AlreadyPending(_) => panic!("request should be new"),
    };
    let old_result = receive_before(&mut loader, Instant::now() + Duration::from_secs(5));
    assert_eq!(old_result.ticket, ticket);

    let replacement = [AIR, DIRT, STONE]
        .into_iter()
        .find(|&block| block != original)
        .unwrap();
    let prepared = world.prepare_edit(x, y, z, replacement).unwrap();
    let expected_version = prepared.new_version;
    world.apply_prepared_edit(prepared).unwrap();
    world
        .get_chunk(ChunkKey {
            x: -40,
            y: 0,
            z: 40,
        })
        .unwrap();

    assert!(
        !world
            .install_loaded_if_absent(old_result.result.unwrap(), ticket.edit_epoch)
            .unwrap()
    );
    assert_eq!(world.get_block(x, y, z).unwrap(), replacement);
    assert_eq!(world.get_chunk(key).unwrap().version, expected_version);
    drop(loader);
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

fn receive_before(loader: &mut ChunkLoader, deadline: Instant) -> ChunkLoadResult {
    loop {
        match loader.try_recv() {
            Ok(result) => return result,
            Err(TryRecvError::Empty) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(2));
            }
            other => panic!("chunk load did not complete: {other:?}"),
        }
    }
}

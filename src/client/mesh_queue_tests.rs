use super::*;
use crate::{
    content::Catalog,
    world::{Chunk, ChunkKey},
};

fn job(x: i32, revision: u64) -> MesherJob {
    let key = ChunkKey { x, y: 5, z: 0 };
    MesherJob {
        chunk: Arc::new(Chunk::from_blocks(
            key,
            1,
            vec![crate::world::AIR; crate::world::CHUNK_VOLUME],
        )),
        known: HashMap::new(),
        catalog: Arc::new(Catalog::builtins()),
        seed: 7,
        revision,
        bounced_gi: false,
    }
}

#[test]
fn hot_edit_coalesces_and_promotes_without_losing_background_progress() {
    let (background, immediate, receiver) = channel();
    assert!(background.try_send(job(0, 1)).is_ok());
    assert!(background.try_send(job(1, 1)).is_ok());
    for revision in 2..=20 {
        assert!(immediate.try_send(job(0, revision)).is_ok());
    }
    assert!(background.try_send(job(0, 2)).is_ok()); // stale submit cannot replace newer work
    assert_eq!(receiver.recv().unwrap().revision, 20);
    for x in 2..=10 {
        assert!(immediate.try_send(job(x, 1)).is_ok());
    }
    assert_eq!(receiver.recv().unwrap().chunk.key.x, 2);
    assert_eq!(receiver.recv().unwrap().chunk.key.x, 3);
    assert_eq!(
        receiver.recv().unwrap().chunk.key.x,
        1,
        "background gets its fourth dispatch even with urgent work waiting"
    );
    drop(background);
    drop(immediate);
    while receiver.recv().is_some() {}
}

#[test]
fn background_backlog_cannot_fill_edit_capacity_and_invalidation_removes_queued_work() {
    let (background, immediate, receiver) = channel();
    for x in 0..64 {
        assert!(background.try_send(job(x, 1)).is_ok());
    }
    assert!(matches!(
        background.try_send(job(65, 1)),
        Err(TrySendError::Full(_))
    ));
    assert!(immediate.try_send(job(65, 2)).is_ok());
    assert_eq!(receiver.recv().unwrap().chunk.key.x, 65);
    for x in 0..64 {
        background.invalidate(ChunkKey { x, y: 5, z: 0 });
    }
    drop(background);
    drop(immediate);
    assert!(receiver.recv().is_none());
}

#[test]
fn shutdown_wakes_idle_workers() {
    let (background, immediate, receiver) = channel();
    let worker = std::thread::spawn(move || assert!(receiver.recv().is_none()));
    drop(background);
    drop(immediate);
    worker.join().unwrap();
}

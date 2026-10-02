use super::*;
use crate::{audio::Command, config::Config};

fn chunks() -> HashMap<ChunkKey, Arc<Chunk>> {
    let key = ChunkKey { x: 0, y: 0, z: 0 };
    HashMap::from([(
        key,
        Arc::new(Chunk::from_blocks(
            key,
            1,
            vec![world::AIR; world::CHUNK_VOLUME],
        )),
    )])
}
fn capture(chunks: &HashMap<ChunkKey, Arc<Chunk>>, generation: u64) -> Capture {
    Capture {
        generation,
        listener: [0.5; 3],
        dependencies: vec![
            (
                ChunkKey { x: 0, y: 0, z: 0 },
                chunks.values().next().cloned(),
            ),
            (ChunkKey { x: 1, y: 0, z: 0 }, None),
        ],
    }
}
fn value(id: u64) -> obstruction::Value {
    obstruction::Value {
        id,
        position: [1.0; 3],
        gain: 0.25,
        lowpass_hz: 1_500.0,
    }
}
fn playing() -> (AudioState, Instant) {
    let mut state = AudioState::new(&Config::default());
    state.install_sounds(crate::audio::sounds::builtin_cached().unwrap());
    state.obstruction.enable_for_test();
    let now = Instant::now();
    state.sounds(
        false,
        None,
        vec![bloxgloom_host_api::sound::Event {
            owner: "bloxgloom".into(),
            voice: "test".into(),
            kind: bloxgloom_host_api::sound::Kind::Play {
                clip: "bloxgloom:break".into(),
                position: [1.0; 3],
                entity: Some(7),
                gain: 1.0,
                pitch: 1.0,
                looping: true,
            },
        }],
        now,
    );
    (state, now)
}

#[test]
fn snapshot_fences_same_revision_replacements_installations_and_evictions() {
    let mut chunks = chunks();
    let snapshot = capture(&chunks, 1);
    assert!(snapshot.current([0.6; 3], &chunks));
    assert!(!snapshot.current([1.0, 0.5, 0.5], &chunks));
    assert!(!snapshot.current([0.99; 3], &chunks));
    let key = ChunkKey { x: 0, y: 0, z: 0 };
    let original = chunks[&key].clone();
    chunks.insert(key, Arc::new(original.as_ref().clone()));
    assert!(
        !snapshot.current([0.5; 3], &chunks),
        "equal revisions still replace terrain"
    );
    chunks.insert(key, original);
    let absent = ChunkKey { x: 1, y: 0, z: 0 };
    chunks.insert(
        absent,
        Arc::new(Chunk::from_blocks(
            absent,
            1,
            vec![world::AIR; world::CHUNK_VOLUME],
        )),
    );
    assert!(
        !snapshot.current([0.5; 3], &chunks),
        "formerly unknown terrain was installed"
    );
    chunks.remove(&absent);
    chunks.remove(&key);
    assert!(!snapshot.current([0.5; 3], &chunks));
}
#[test]
fn queue_pressure_retries_but_an_edit_or_teleport_discards_the_result() {
    let chunks = chunks();
    let (mut state, now) = playing();
    state.obstruction.ready = Some(Ready {
        capture: capture(&chunks, 1),
        values: vec![value(1)],
    });
    state.blocked.set(true);
    state.apply_ready_obstruction([0.5; 3], &chunks, now);
    assert!(state.obstruction.ready.is_some());
    state.blocked.set(false);
    state.apply_ready_obstruction([0.5; 3], &chunks, now);
    assert!(
        matches!(state.sent.borrow().last(), Some(Command::PlayObstructed { transmission, .. }) if *transmission == 0.25)
    );
    assert!(state.obstruction.ready.is_none());
    state.sent.borrow_mut().clear();
    state.obstruction.ready = Some(Ready {
        capture: capture(&chunks, 2),
        values: vec![value(1)],
    });
    state.apply_ready_obstruction([10.5; 3], &chunks, now);
    assert!(state.sent.borrow().is_empty());
    state.obstruction.ready = Some(Ready {
        capture: capture(&chunks, 3),
        values: vec![value(1)],
    });
    state.apply_ready_obstruction([0.5; 3], &HashMap::new(), now);
    assert!(state.sent.borrow().is_empty());
}
#[test]
fn late_session_results_cannot_consume_the_new_pending_capture() {
    let chunks = chunks();
    let mut state = State::new();
    state.pending = Some(capture(&chunks, 1));
    state.generation = 1;
    state.retire();
    state.pending = Some(capture(&chunks, 2));
    state.accept(obstruction::ResultBatch {
        generation: 1,
        listener: [0.5; 3],
        values: vec![value(1)],
    });
    assert_eq!(state.pending.as_ref().unwrap().generation, 2);
    assert!(state.ready.is_none());
    state.accept(obstruction::ResultBatch {
        generation: 2,
        listener: [0.5; 3],
        values: vec![value(2)],
    });
    assert!(state.pending.is_none());
    assert_eq!(state.ready.unwrap().values[0].id, 2);
}

#[test]
fn missing_result_times_out_and_resubmits_without_wedging_playback() {
    let chunks = chunks();
    let (mut state, now) = playing();
    state.obstruction.pending = Some(capture(&chunks, 19));
    state.obstruction.generation = 19;
    state.obstruction.last_submit = Some(now);
    let catalog = Arc::new(Catalog::builtins());
    state.poll_obstruction([0.5; 3], &chunks, &catalog, now + RESULT_TIMEOUT);
    assert!(
        matches!(state.sent.borrow().last(), Some(Command::PlayObstructed { transmission, .. }) if *transmission == 0.35)
    );
    assert_eq!(state.obstruction.pending.as_ref().unwrap().generation, 20);
    state.obstruction.accept(obstruction::ResultBatch {
        generation: 19,
        listener: [0.5; 3],
        values: vec![value(1)],
    });
    assert_eq!(state.obstruction.pending.as_ref().unwrap().generation, 20);
    assert!(state.obstruction.ready.is_none());
    state.sent.borrow_mut().clear();
    state.obstruction.pending = Some(capture(&chunks, 999));
    state.obstruction.last_submit = Some(now);
    state.poll_obstruction([0.5; 3], &chunks, &catalog, now + RESULT_TIMEOUT * 2);
    assert!(
        matches!(state.sent.borrow().last(), Some(Command::Obstruction { gain, .. }) if *gain == 0.35)
    );
}
#[test]
fn real_worker_delivers_initial_profile_and_refreshes_after_wall_edit() {
    let (mut state, now) = playing();
    let catalog = Arc::new(Catalog::builtins());
    let mut chunks = chunks();
    // Place a wall between a known listener and source without blocking anchors.
    let key = ChunkKey { x: 0, y: 0, z: 0 };
    let mut blocks = vec![world::AIR; world::CHUNK_VOLUME];
    for y in 0..16 {
        for z in 0..16 {
            blocks[y * 256 + z * 16 + 2] = world::STONE;
        }
    }
    chunks.insert(key, Arc::new(Chunk::from_blocks(key, 1, blocks)));
    let listener = [4.5, 1.0, 1.0];
    state.poll_obstruction(listener, &chunks, &catalog, now);
    let deadline = Instant::now() + Duration::from_secs(2);
    while state.sent.borrow().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
        state.poll_obstruction(listener, &chunks, &catalog, now + Duration::from_millis(1));
    }
    assert!(
        matches!(state.sent.borrow().last(), Some(Command::PlayObstructed { transmission, .. }) if *transmission < 0.3)
    );
    state.sent.borrow_mut().clear();
    chunks.insert(
        key,
        Arc::new(Chunk::from_blocks(
            key,
            1,
            vec![world::AIR; world::CHUNK_VOLUME],
        )),
    );
    state.poll_obstruction(listener, &chunks, &catalog, now + INTERVAL);
    let deadline = Instant::now() + Duration::from_secs(2);
    while state.sent.borrow().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
        state.poll_obstruction(
            listener,
            &chunks,
            &catalog,
            now + INTERVAL + Duration::from_millis(1),
        );
    }
    assert!(
        matches!(state.sent.borrow().last(), Some(Command::Obstruction { gain, lowpass_hz, .. }) if *gain == 1.0 && *lowpass_hz == 20_000.0)
    );
}

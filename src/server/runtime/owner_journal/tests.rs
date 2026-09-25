use super::*;
use crate::server::parallel::OwnerKey;
use crate::server::registry::SystemId;
use crate::world::ChunkKey;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct U64Codec;

impl super::super::owner_codec::OwnerValueCodec for U64Codec {
    fn decode(
        &self,
        payload: &[u8],
    ) -> Result<OwnerData, super::super::owner_codec::OwnerCodecError> {
        if payload.len() != 8 {
            return Err(super::super::owner_codec::OwnerCodecError::InvalidData);
        }
        Ok(OwnerData::new(u64::from_le_bytes(
            payload.try_into().expect("checked length"),
        )))
    }

    fn encode(
        &self,
        value: &OwnerData,
    ) -> Result<Vec<u8>, super::super::owner_codec::OwnerCodecError> {
        value
            .get::<u64>()
            .map(|value| value.to_le_bytes().to_vec())
            .ok_or(super::super::owner_codec::OwnerCodecError::InvalidData)
    }
}

struct BytesCodec;

impl super::super::owner_codec::OwnerValueCodec for BytesCodec {
    fn decode(
        &self,
        payload: &[u8],
    ) -> Result<OwnerData, super::super::owner_codec::OwnerCodecError> {
        Ok(OwnerData::new(payload.to_vec()))
    }

    fn encode(
        &self,
        value: &OwnerData,
    ) -> Result<Vec<u8>, super::super::owner_codec::OwnerCodecError> {
        value
            .get::<Vec<u8>>()
            .cloned()
            .ok_or(super::super::owner_codec::OwnerCodecError::InvalidData)
    }
}

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-owner-journal-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn counters() -> Vec<OwnerSystemConfig> {
    vec![
        OwnerSystemConfig::new(
            SystemId::new("test:counters").unwrap(),
            Arc::new(U64Codec),
            1,
            8,
        )
        .unwrap(),
    ]
}

fn chunk(x: i32) -> OwnerKey {
    OwnerKey::Chunk(ChunkKey { x, y: 0, z: 0 })
}

fn value(runtime: &DurableOwnerRuntime, owner: OwnerKey) -> (u64, u64) {
    let id = SystemId::new("test:counters").unwrap();
    let (revision, data) = runtime.snapshot(&id, owner).unwrap();
    (revision, *data.get::<u64>().unwrap())
}

#[test]
fn mutated_cells_survive_restart_without_a_clean_shutdown() {
    let dir = TestDir::new();
    let id = SystemId::new("test:counters").unwrap();
    {
        let mut runtime = DurableOwnerRuntime::open(&dir.0, counters()).unwrap();
        runtime
            .insert(&id, chunk(0), OwnerData::new(10u64), 1)
            .unwrap();
        runtime
            .insert(&id, chunk(1), OwnerData::new(20u64), 1)
            .unwrap();
        assert_eq!(
            runtime
                .commit_wave(
                    &id,
                    vec![OwnerWrite::new(chunk(0), 0, OwnerData::new(11u64))],
                    2
                )
                .unwrap(),
            1
        );
        assert_eq!(value(&runtime, chunk(0)), (1, 11));
        // A prepared-but-never-submitted wave must not survive: it never
        // reached the WAL, so recovery must not see it.
        let unsubmitted = runtime
            .store
            .prepare(
                &id,
                vec![OwnerWrite::new(chunk(0), 1, OwnerData::new(99u64))],
            )
            .unwrap();
        drop(unsubmitted);
        // No checkpoint, no rotation, no graceful close: the runtime is
        // simply dropped, the way a crashed process leaves its WAL tail.
    }
    let runtime = DurableOwnerRuntime::open(&dir.0, counters()).unwrap();
    // Exactly the last receipted state: no more, no less.
    assert_eq!(value(&runtime, chunk(0)), (1, 11));
    assert_eq!(value(&runtime, chunk(1)), (0, 20));
    assert_eq!(runtime.cell_count(), 2);
}

#[test]
fn interrupted_commit_recovers_to_the_last_complete_record() {
    let dir = TestDir::new();
    let id = SystemId::new("test:counters").unwrap();
    {
        let mut runtime = DurableOwnerRuntime::open(&dir.0, counters()).unwrap();
        runtime
            .insert(&id, chunk(0), OwnerData::new(10u64), 1)
            .unwrap();
        runtime
            .commit_wave(
                &id,
                vec![OwnerWrite::new(chunk(0), 0, OwnerData::new(11u64))],
                2,
            )
            .unwrap();
        // Crash between WAL append and apply: the record is synced (receipt
        // received) but the in-memory commit never runs.
        let staged = runtime
            .stage(
                &id,
                vec![OwnerWrite::new(chunk(0), 1, OwnerData::new(12u64))],
                3,
            )
            .unwrap();
        let receipt = staged.receiver.recv().unwrap().unwrap();
        assert!(!receipt.duplicate);
        drop(staged.prepared);
        drop(runtime);
    }
    // Recovery lands on the last complete record: the interrupted wave's
    // after-value is durable even though the crashed process never applied
    // it, and there is no half-applied state.
    let runtime = DurableOwnerRuntime::open(&dir.0, counters()).unwrap();
    assert_eq!(value(&runtime, chunk(0)), (2, 12));
    assert_eq!(runtime.cell_count(), 1);
}

#[test]
fn oversized_waves_defer_without_touching_state_or_the_journal() {
    let dir = TestDir::new();
    let id = SystemId::new("test:blobs").unwrap();
    let configs =
        vec![OwnerSystemConfig::new(id.clone(), Arc::new(BytesCodec), 1, 64 * 1024).unwrap()];
    let mut runtime = DurableOwnerRuntime::open(&dir.0, configs).unwrap();
    runtime
        .insert(&id, chunk(0), OwnerData::new(vec![1u8; 1_024]), 1)
        .unwrap();
    let writes: Vec<OwnerWrite> = (0..10)
        .map(|_| OwnerWrite::new(chunk(0), 0, OwnerData::new(vec![2u8; 60 * 1_024])))
        .collect();
    // Ten replacements for one owner: prepare rejects duplicates first.
    let error = runtime.stage(&id, writes, 2).unwrap_err();
    assert_ne!(error.kind(), ErrorKind::InvalidData);

    for x in 1..10 {
        runtime
            .insert(&id, chunk(x), OwnerData::new(vec![3u8; 1_024]), 1)
            .unwrap();
    }
    let writes: Vec<OwnerWrite> = (1..10)
        .map(|x| OwnerWrite::new(chunk(x), 0, OwnerData::new(vec![4u8; 60 * 1_024])))
        .collect();
    let error = runtime.stage(&id, writes, 2).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_ne!(error.kind(), ErrorKind::InvalidData);
    drop(runtime);

    let configs = vec![
        OwnerSystemConfig::new(
            SystemId::new("test:blobs").unwrap(),
            Arc::new(BytesCodec),
            1,
            64 * 1024,
        )
        .unwrap(),
    ];
    let runtime = DurableOwnerRuntime::open(&dir.0, configs).unwrap();
    let (_, data) = runtime.snapshot(&id, chunk(1)).unwrap();
    assert_eq!(data.get::<Vec<u8>>().unwrap(), &vec![3u8; 1_024]);
}

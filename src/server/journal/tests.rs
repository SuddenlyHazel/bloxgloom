use super::*;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

static TEST_ID: AtomicU64 = AtomicU64::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-journal-{}-{}",
            std::process::id(),
            TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn file(&self) -> PathBuf {
        self.0.join("journal.wal")
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn transaction(id: u128, tick: u64) -> Transaction {
    Transaction::new(
        id,
        tick,
        vec![
            Change::new(
                StateKey::new("bloxgloom:inventory", 42u128.to_le_bytes().to_vec()),
                b"old inventory".to_vec(),
                b"new inventory".to_vec(),
            ),
            Change::new(
                StateKey::new(
                    "bloxgloom:chunk_snapshot",
                    [
                        (-1i32).to_le_bytes(),
                        2i32.to_le_bytes(),
                        3i32.to_le_bytes(),
                    ]
                    .concat(),
                ),
                b"old chunk".to_vec(),
                b"new chunk".to_vec(),
            ),
        ],
    )
}

fn committed_frame(tx: &Transaction) -> Vec<u8> {
    encode_frame(&tx.clone().canonicalize().unwrap()).unwrap()
}

fn state_tx(id: u128, before: &[u8], after: &[u8]) -> Transaction {
    Transaction::new(
        id,
        id as u64,
        vec![Change::new(
            StateKey::new("bloxgloom:test_state", b"main".to_vec()),
            before.to_vec(),
            after.to_vec(),
        )],
    )
}

fn write_one_transaction(path: &PathBuf, tx: Transaction) -> CommitReceipt {
    let writer = Journal::open(path)
        .unwrap()
        .into_writer(8, Duration::ZERO)
        .unwrap();
    let receiver = writer.try_submit(tx).unwrap();
    let receipt = receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    writer.shutdown().unwrap();
    receipt
}

fn append_direct(journal: &mut Journal, tx: Transaction) -> io::Result<CommitReceipt> {
    let (acknowledge, receiver) = mpsc::channel();
    let transaction = tx.canonicalize()?;
    let result = journal
        .append_batch(vec![Request {
            reserved_bytes: frame_len(&transaction),
            transaction,
            acknowledge,
        }])
        .pop()
        .expect("one append result")?;
    let _ = receiver;
    Ok(result)
}

mod append_recovery;
mod drop_compaction;
#[path = "tests/rotation.rs"]
mod rotation_tests;
mod writer;

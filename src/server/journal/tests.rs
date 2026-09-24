use super::*;
use std::fs::{self, OpenOptions};
use std::io::Write;
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

#[test]
fn crc32_matches_standard_check_value() {
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
}

#[test]
fn writer_acknowledges_only_a_synced_transaction_and_reopens_it() {
    let dir = TestDir::new();
    let journal = Journal::open(dir.file()).unwrap();
    let writer = journal.into_writer(8, Duration::from_millis(5)).unwrap();
    let tx = transaction(1, 27);
    let receiver = writer.try_submit(tx.clone()).unwrap();
    let receipt = receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(receipt.id, tx.id);
    assert_eq!(receipt.sequence, 1);
    assert!(!receipt.duplicate);
    writer.shutdown().unwrap();

    let reopened = Journal::open(dir.file()).unwrap();
    assert_eq!(reopened.records(), &[tx.clone().canonicalize().unwrap()]);
    let mut replayed = Vec::new();
    reopened
        .replay(|record| {
            replayed.push((record.id, record.tick));
            Ok(())
        })
        .unwrap();
    assert_eq!(replayed, [(1, 27)]);
}

#[test]
fn replay_deduplicates_identical_ids_and_rejects_conflicting_reuse() {
    let dir = TestDir::new();
    let path = dir.file();
    let tx = transaction(8, 30);
    Journal::open(&path).unwrap();
    {
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        let frame = committed_frame(&tx);
        file.write_all(&frame).unwrap();
        file.write_all(&frame).unwrap();
        file.sync_all().unwrap();
    }
    let reopened = Journal::open(&path).unwrap();
    assert_eq!(reopened.records().len(), 1);
    let mut count = 0;
    reopened
        .replay(|_| {
            count += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(count, 1);

    let dir = TestDir::new();
    let path = dir.file();
    Journal::open(&path).unwrap();
    let mut conflicting = transaction(8, 30);
    conflicting.changes[0].after = b"different committed value".to_vec();
    {
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&committed_frame(&tx)).unwrap();
        file.write_all(&committed_frame(&conflicting)).unwrap();
        file.sync_all().unwrap();
    }
    assert_eq!(
        Journal::open(path).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn redo_finishes_a_partially_written_transaction_and_is_idempotent() {
    let transaction = Transaction::new(
        9,
        31,
        vec![
            Change::new(
                StateKey::new("test:a", vec![1]),
                b"before-a".to_vec(),
                b"after-a".to_vec(),
            ),
            Change::new(
                StateKey::new("test:b", vec![2]),
                b"before-b".to_vec(),
                b"after-b".to_vec(),
            ),
        ],
    );
    let mut state = HashMap::from([
        (StateKey::new("test:a", vec![1]), b"before-a".to_vec()),
        (StateKey::new("test:b", vec![2]), b"before-b".to_vec()),
    ]);
    let mut fail_b_once = true;
    let first = transaction.redo(
        &mut state,
        |state, key| Ok(state.get(key).cloned().unwrap_or_default()),
        |state, key, after| {
            if key.domain == "test:b" && fail_b_once {
                fail_b_once = false;
                return Err(io::Error::other("injected crash during transaction replay"));
            }
            state.insert(key.clone(), after.to_vec());
            Ok(())
        },
    );
    assert!(first.is_err());
    assert_eq!(state[&StateKey::new("test:a", vec![1])], b"after-a");
    assert_eq!(state[&StateKey::new("test:b", vec![2])], b"before-b");

    transaction
        .redo(
            &mut state,
            |state, key| Ok(state.get(key).cloned().unwrap_or_default()),
            |state, key, after| {
                state.insert(key.clone(), after.to_vec());
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(state[&StateKey::new("test:a", vec![1])], b"after-a");
    assert_eq!(state[&StateKey::new("test:b", vec![2])], b"after-b");

    let mut writes = 0;
    transaction
        .redo(
            &mut state,
            |state, key| Ok(state.get(key).cloned().unwrap_or_default()),
            |_, _, _| {
                writes += 1;
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(writes, 0);
}

#[test]
fn all_incomplete_append_prefixes_recover_to_the_last_complete_record() {
    let dir = TestDir::new();
    let tx = transaction(10, 100);
    let frame = committed_frame(&tx);
    let header = {
        let journal = Journal::open(dir.file()).unwrap();
        drop(journal);
        fs::read(dir.file()).unwrap()
    };

    for cut in 0..=frame.len() {
        let path = dir.0.join(format!("cut-{cut}.wal"));
        let mut bytes = header.clone();
        bytes.extend_from_slice(&frame[..cut]);
        fs::write(&path, bytes).unwrap();
        let journal = Journal::open(&path).unwrap();
        assert_eq!(journal.records().len(), usize::from(cut == frame.len()));
        let expected_len = FILE_HEADER_LEN + if cut == frame.len() { frame.len() } else { 0 };
        assert_eq!(fs::metadata(path).unwrap().len(), expected_len as u64);
    }
}

#[test]
fn complete_corrupt_record_and_invalid_header_are_rejected() {
    let dir = TestDir::new();
    let path = dir.file();
    let tx = transaction(12, 44);
    let mut bytes = {
        let journal = Journal::open(&path).unwrap();
        drop(journal);
        fs::read(&path).unwrap()
    };
    let mut frame = committed_frame(&tx);
    frame[8] ^= 0x40; // Change committed payload without changing its checksum.
    bytes.extend(frame);
    fs::write(&path, bytes).unwrap();
    assert_eq!(
        Journal::open(&path).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );

    let path = dir.0.join("bad-header.wal");
    fs::write(&path, b"BGWJ\x01\x00\x00\x00\x00\x00").unwrap();
    assert_eq!(
        Journal::open(path).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn exact_legacy_header_prefixes_are_repaired_and_nonprefixes_fail_closed() {
    let dir = TestDir::new();
    let path = dir.file();
    let header = legacy_header();
    for cut in 0..FILE_HEADER_LEN {
        let partial = dir.0.join(format!("partial-{cut}.wal"));
        fs::write(&partial, &header[..cut]).unwrap();
        let journal = Journal::open(&partial).unwrap();
        assert_eq!(journal.sequence(), 0);
        assert_eq!(fs::read(&partial).unwrap(), header);
    }

    let corrupt = dir.0.join("nonprefix.wal");
    fs::write(&corrupt, b"X").unwrap();
    assert_eq!(
        Journal::open(corrupt).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );

    // A crash before an orphan temporary file is renamed leaves the canonical
    // path absent. Opening creates a complete canonical header and ignores it.
    fs::write(dir.0.join("journal.wal.tmp.crashed"), b"BGWJ\x01").unwrap();
    let created = Journal::open(path).unwrap();
    assert_eq!(created.bytes(), FILE_HEADER_LEN as u64);
}

#[test]
fn rotation_switch_is_ordered_nonblocking_and_preserves_state_and_id_watermarks() {
    let dir = TestDir::new();
    let path = dir.file();
    let state_key = StateKey::new("bloxgloom:test_state", b"main".to_vec());
    let receipt_key = StateKey::new(
        "bloxgloom:action_receipt",
        [7u128.to_le_bytes(), 9u128.to_le_bytes()].concat(),
    );
    let tombstone_key = StateKey::new("bloxgloom:test_tombstone", b"removed".to_vec());
    let first_tx = Transaction::new(
        1,
        10,
        vec![
            Change::new(state_key.clone(), b"zero".to_vec(), b"one".to_vec()),
            Change::new(receipt_key.clone(), Vec::new(), vec![1]),
            Change::new(tombstone_key.clone(), vec![9], Vec::new()),
        ],
    );
    let writer = Journal::open(&path)
        .unwrap()
        .into_writer(8, Duration::ZERO)
        .unwrap();
    let committed = writer
        .try_submit(first_tx.clone())
        .unwrap()
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(committed.sequence, 1);
    assert_eq!(writer.sequence(), 1);

    let wrong = writer.try_rotate(0).unwrap();
    assert_eq!(
        wrong
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let rotation = writer
        .try_rotate(1)
        .unwrap()
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(rotation.cut_sequence, 1);
    assert_eq!(rotation.generation, 1);
    assert_eq!(writer.sequence(), 1);
    assert!(writer.bytes() < JOURNAL_ROTATION_SOFT_LIMIT_BYTES);
    writer.shutdown().unwrap();

    // The legacy path may be cleaned only after the new manifest is durable;
    // reopening follows the manifest and sees a compacted base.
    assert!(!path.exists());
    let mut reopened = Journal::open(&path).unwrap();
    assert_eq!(reopened.sequence(), 1);
    assert_eq!(reopened.next_id().unwrap(), 2);
    assert!(reopened.records().is_empty());
    assert_eq!(reopened.latest_values()[&receipt_key], [1]);
    assert!(reopened.latest_values()[&tombstone_key].is_empty());
    assert!(reopened.validate_snapshot(&state_key, b"one").is_ok());
    assert!(reopened.validate_snapshot(&state_key, b"zero").is_err());

    let next_tx = state_tx(2, b"one", b"two");
    assert_eq!(
        append_direct(&mut reopened, first_tx.clone())
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        append_direct(&mut reopened, next_tx.clone())
            .unwrap()
            .sequence,
        2
    );
    assert_eq!(reopened.sequence(), 2);
    assert_eq!(reopened.next_id().unwrap(), 3);
    assert_eq!(reopened.records().len(), 1);
    reopened.rotate(2).unwrap();
    assert_eq!(reopened.records().len(), 0);
    drop(reopened);

    let reopened = Journal::open(path).unwrap();
    assert_eq!(reopened.sequence(), 2);
    assert_eq!(reopened.next_id().unwrap(), 3);
    assert_eq!(reopened.latest_values()[&state_key], b"two");
    assert_eq!(reopened.latest_values()[&receipt_key], [1]);
    assert!(reopened.latest_values()[&tombstone_key].is_empty());
    assert!(reopened.validate_snapshot(&state_key, b"one").is_err());
    assert!(reopened.validate_snapshot(&state_key, b"two").is_ok());
    assert!(reopened.records().is_empty());
}

#[test]
fn restart_selects_old_or_new_generation_at_each_rotation_crash_boundary() {
    let points = [
        rotation::CrashPoint::BaseTempSynced,
        rotation::CrashPoint::TailTempSynced,
        rotation::CrashPoint::BaseInstalled,
        rotation::CrashPoint::TailInstalled,
        rotation::CrashPoint::ManifestTempSynced,
        rotation::CrashPoint::ManifestInstalled,
    ];
    for point in points {
        let dir = TestDir::new();
        let path = dir.file();
        let tx = state_tx(1, b"zero", b"one");
        write_one_transaction(&path, tx);
        let mut journal = Journal::open(&path).unwrap();
        let failure = journal.rotate_crashing_at(1, point);
        assert!(failure.is_err(), "{point:?} should simulate a crash");
        assert_eq!(
            append_direct(&mut journal, state_tx(2, b"one", b"two"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::Other,
            "{point:?} must fail-stop appends after rotation I/O error"
        );
        drop(journal);

        let reopened = Journal::open(&path).unwrap();
        assert_eq!(reopened.sequence(), 1, "{point:?}");
        if point == rotation::CrashPoint::ManifestInstalled {
            assert_eq!(reopened.generation, 1);
            assert!(reopened.records().is_empty());
            assert_eq!(
                reopened.latest_values()[&StateKey::new("bloxgloom:test_state", b"main".to_vec())],
                b"one"
            );
        } else {
            assert_eq!(reopened.generation, 0);
            assert_eq!(reopened.records().len(), 1);
        }
    }
}

#[test]
fn corrupt_or_missing_manifest_references_fail_closed_without_legacy_fallback() {
    for damage in ["missing-base", "corrupt-base", "missing-tail"] {
        let dir = TestDir::new();
        let path = dir.file();
        write_one_transaction(&path, state_tx(1, b"zero", b"one"));
        let mut journal = Journal::open(&path).unwrap();
        journal.rotate(1).unwrap();
        drop(journal);

        let manifest = rotation::read_manifest(&path).unwrap();
        match damage {
            "missing-base" => {
                fs::remove_file(rotation::base_path(&path, &manifest).unwrap()).unwrap()
            }
            "corrupt-base" => {
                let base = rotation::base_path(&path, &manifest).unwrap();
                let mut bytes = fs::read(&base).unwrap();
                bytes[8] ^= 1;
                fs::write(base, bytes).unwrap();
            }
            "missing-tail" => {
                fs::remove_file(rotation::tail_path(&path, &manifest).unwrap()).unwrap()
            }
            _ => unreachable!(),
        }
        assert_eq!(
            Journal::open(&path).err().unwrap().kind(),
            io::ErrorKind::InvalidData,
            "{damage}"
        );
    }
}

#[test]
fn rotated_tail_recovers_partial_frames_and_deduplicates_complete_retries() {
    let dir = TestDir::new();
    let path = dir.file();
    write_one_transaction(&path, state_tx(1, b"zero", b"one"));
    let mut journal = Journal::open(&path).unwrap();
    journal.rotate(1).unwrap();
    drop(journal);

    let manifest = rotation::read_manifest(&path).unwrap();
    let tail = rotation::tail_path(&path, &manifest).unwrap();
    let start_len = fs::metadata(&tail).unwrap().len();
    let tx = state_tx(2, b"one", b"two");
    let frame = committed_frame(&tx);
    {
        let mut file = OpenOptions::new().append(true).open(&tail).unwrap();
        file.write_all(&frame).unwrap();
        file.write_all(&frame).unwrap();
        file.write_all(&frame[..5]).unwrap();
        file.sync_all().unwrap();
    }

    let reopened = Journal::open(&path).unwrap();
    assert_eq!(reopened.sequence(), 3);
    assert_eq!(reopened.next_id().unwrap(), 3);
    assert_eq!(reopened.records(), &[tx.clone().canonicalize().unwrap()]);
    assert_eq!(
        reopened.latest_values()[&StateKey::new("bloxgloom:test_state", b"main".to_vec())],
        b"two"
    );
    assert_eq!(
        fs::metadata(&tail).unwrap().len(),
        start_len + (frame.len() * 2) as u64
    );
}

#[test]
fn writer_treats_repeated_ids_idempotently_and_rejects_conflicts() {
    let dir = TestDir::new();
    let writer = Journal::open(dir.file())
        .unwrap()
        .into_writer(8, Duration::from_millis(25))
        .unwrap();
    let tx = transaction(20, 77);
    let first = writer.try_submit(tx.clone()).unwrap();
    let second = writer.try_submit(tx.clone()).unwrap();
    let mut conflicting = tx.clone();
    conflicting.changes[0].after = b"not the same transaction".to_vec();
    let conflict = writer.try_submit(conflicting).unwrap();

    let first = first.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
    let second = second
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert!(!first.duplicate);
    assert!(second.duplicate);
    assert_eq!(first.sequence, second.sequence);
    assert_eq!(
        conflict
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    writer.shutdown().unwrap();
    assert_eq!(Journal::open(dir.file()).unwrap().records().len(), 1);
}

#[test]
fn writer_queue_is_bounded_and_submission_does_not_wait_for_disk() {
    let dir = TestDir::new();
    let writer = Journal::open(dir.file())
        .unwrap()
        .into_writer(1, Duration::from_millis(200))
        .unwrap();
    let mut accepted = Vec::new();
    let mut saw_full = false;
    for id in 1..100 {
        let tx = Transaction::new(
            id,
            id as u64,
            vec![Change::new(
                StateKey::new("bloxgloom:inventory", id.to_le_bytes().to_vec()),
                Vec::new(),
                vec![id as u8],
            )],
        );
        match writer.try_submit(tx) {
            Ok(receiver) => accepted.push(receiver),
            Err(SubmitError::Full) => {
                saw_full = true;
                break;
            }
            Err(error) => panic!("unexpected submit error: {error}"),
        }
    }
    assert!(saw_full, "bounded request queue accepted every request");
    for receiver in accepted {
        receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap();
    }
    writer.shutdown().unwrap();
}

#[test]
fn oversized_or_ambiguous_transactions_are_rejected_before_queueing() {
    let dir = TestDir::new();
    let writer = Journal::open(dir.file())
        .unwrap()
        .into_writer(2, Duration::ZERO)
        .unwrap();
    let invalid = Transaction::new(
        1,
        0,
        vec![
            Change::new(
                StateKey::new("bloxgloom:inventory", vec![1]),
                b"a".to_vec(),
                b"b".to_vec(),
            ),
            Change::new(
                StateKey::new("bloxgloom:inventory", vec![1]),
                b"b".to_vec(),
                b"c".to_vec(),
            ),
        ],
    );
    assert!(matches!(
        writer.try_submit(invalid),
        Err(SubmitError::Invalid(_))
    ));

    let oversized = Transaction::new(
        2,
        0,
        vec![Change::new(
            StateKey::new("bloxgloom:drop", vec![1]),
            vec![0; MAX_TRANSACTION_BYTES],
            vec![1; MAX_TRANSACTION_BYTES],
        )],
    );
    assert!(matches!(
        writer.try_submit(oversized),
        Err(SubmitError::Invalid(_))
    ));
    writer.shutdown().unwrap();
}

#[test]
fn wal_reservation_accounts_for_queued_frames_before_the_worker_sees_them() {
    let dir = TestDir::new();
    let writer = Journal::open(dir.file())
        .unwrap()
        .into_writer(2, Duration::ZERO)
        .unwrap();
    let tx = Transaction::new(
        1,
        1,
        vec![Change::new(
            StateKey::new("bloxgloom:inventory", 1u128.to_le_bytes()),
            vec![1, 2, 3],
            vec![4, 5, 6, 7],
        )],
    )
    .canonicalize()
    .unwrap();
    assert_eq!(frame_len(&tx) as usize, encode_frame(&tx).unwrap().len());
    writer
        .projected_usage
        .store(MAX_JOURNAL_BYTES - frame_len(&tx) + 1, Ordering::Release);
    assert!(writer.needs_rotation());
    assert!(matches!(writer.try_submit(tx), Err(SubmitError::Full)));
    assert_eq!(writer.bytes(), FILE_HEADER_LEN as u64);
    writer.shutdown().unwrap();
}

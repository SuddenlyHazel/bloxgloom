use super::*;
use std::{fs, fs::OpenOptions, io, io::Write, time::Duration};

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
        rotation::CrashPoint::BasePartial,
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

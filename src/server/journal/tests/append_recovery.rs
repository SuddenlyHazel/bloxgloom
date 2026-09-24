use super::*;
use std::{fs, fs::OpenOptions, io::Write, time::Duration};

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

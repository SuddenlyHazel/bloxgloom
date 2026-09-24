use super::*;
use std::{io, time::Duration};

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

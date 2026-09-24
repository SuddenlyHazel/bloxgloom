use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

fn key(name: &str) -> StateKey {
    StateKey::new(
        format!("bloxgloom:test_{name}"),
        7u64.to_le_bytes().to_vec(),
    )
}

fn key_on_shard(shard: usize, count: usize) -> StateKey {
    (0..100)
        .map(|index| key(&format!("shard_{index}")))
        .find(|key| checkpoint_shard(key, count) == shard)
        .expect("fixture keys cover both shards")
}

fn receive(writer: &CheckpointWriter) -> CheckpointReceipt {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match writer.try_recv() {
            Ok(receipt) => return receipt,
            Err(TryRecvError::Empty) if Instant::now() < deadline => thread::yield_now(),
            Err(error) => panic!("checkpoint receipt unavailable: {error}"),
        }
    }
}

#[test]
fn capacity_counts_running_jobs_and_unconsumed_receipts() {
    let writer = CheckpointWriter::new(1);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    writer
        .try_submit(key("saturation"), 1, vec![1], move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        writer.try_submit(key("saturation"), 2, vec![2], |_| Ok(())),
        Err(CheckpointSubmitError::Full)
    );

    release_tx.send(()).unwrap();
    let receipt = receive(&writer);
    assert!(receipt.result.is_ok());
    assert_eq!(receipt.revision, 1);

    // The first receipt freed the only outstanding slot.
    writer
        .try_submit(key("saturation"), 2, vec![2], |_| Ok(()))
        .unwrap();
    assert!(receive(&writer).result.is_ok());
}

#[test]
fn completed_but_unconsumed_receipt_still_occupies_capacity() {
    let writer = CheckpointWriter::new(2);
    let (second_started_tx, second_started_rx) = mpsc::channel();
    let (release_second_tx, release_second_rx) = mpsc::channel();
    writer
        .try_submit(key("receipt_bound"), 1, vec![1], |_| Ok(()))
        .unwrap();
    writer
        .try_submit(key("receipt_bound"), 2, vec![2], move |_| {
            // The single worker reaches this closure only after it has written
            // the first completion into the receipt channel.
            second_started_tx.send(()).unwrap();
            release_second_rx.recv().unwrap();
            Ok(())
        })
        .unwrap();
    second_started_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap();

    assert_eq!(
        writer.try_submit(key("receipt_bound"), 3, vec![3], |_| Ok(())),
        Err(CheckpointSubmitError::Full)
    );
    release_second_tx.send(()).unwrap();
    assert_eq!(receive(&writer).revision, 1);
    assert_eq!(receive(&writer).revision, 2);

    writer
        .try_submit(key("receipt_bound"), 3, vec![3], |_| Ok(()))
        .unwrap();
    assert_eq!(receive(&writer).revision, 3);
}

#[test]
fn same_key_snapshots_are_written_in_fifo_submission_order() {
    let writer = CheckpointWriter::new(8);
    let order = Arc::new(Mutex::new(Vec::new()));
    for revision in 1..=8 {
        let order = Arc::clone(&order);
        writer
            .try_submit(
                key("fifo"),
                revision,
                vec![revision as u8],
                move |snapshot| {
                    order.lock().unwrap().push(snapshot[0]);
                    Ok(())
                },
            )
            .unwrap();
    }

    let mut writer = writer;
    writer.shutdown().unwrap();
    let mut revisions = Vec::new();
    for _ in 0..8 {
        let receipt = writer.try_recv().unwrap();
        assert!(receipt.result.is_ok());
        revisions.push(receipt.revision as u8);
    }
    assert_eq!(*order.lock().unwrap(), (1..=8).collect::<Vec<_>>());
    assert_eq!(revisions, (1..=8).collect::<Vec<_>>());
    assert_eq!(
        writer.try_submit(key("fifo"), 9, vec![9], |_| Ok(())),
        Err(CheckpointSubmitError::Closed)
    );
}

#[test]
fn receipt_is_not_published_until_write_closure_returns() {
    let writer = CheckpointWriter::new(1);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let finished = Arc::new(AtomicBool::new(false));
    let closure_finished = Arc::clone(&finished);
    writer
        .try_submit(key("ack"), 4, vec![4], move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            closure_finished.store(true, Ordering::Release);
            Ok(())
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(writer.try_recv().unwrap_err(), TryRecvError::Empty);
    release_tx.send(()).unwrap();

    let receipt = receive(&writer);
    assert!(finished.load(Ordering::Acquire));
    assert_eq!(receipt.revision, 4);
    assert!(receipt.result.is_ok());
}

#[test]
fn failed_write_is_returned_with_its_key_and_revision() {
    let writer = CheckpointWriter::new(1);
    let expected_key = key("failure");
    writer
        .try_submit(expected_key.clone(), 12, vec![0], |_| {
            Err(io::Error::new(io::ErrorKind::StorageFull, "disk full"))
        })
        .unwrap();

    let receipt = receive(&writer);
    assert_eq!(receipt.key, expected_key);
    assert_eq!(receipt.revision, 12);
    assert_eq!(receipt.result.unwrap_err().to_string(), "disk full");
}

#[test]
fn closure_panic_becomes_an_error_receipt_and_worker_keeps_running() {
    let writer = CheckpointWriter::new(2);
    writer
        .try_submit(key("panic"), 1, vec![], |_| -> io::Result<()> {
            panic!("simulated checkpoint failure")
        })
        .unwrap();
    writer
        .try_submit(key("after_panic"), 2, vec![], |_| Ok(()))
        .unwrap();

    let failed = receive(&writer);
    assert!(
        failed
            .result
            .unwrap_err()
            .to_string()
            .contains("simulated checkpoint failure")
    );
    assert!(receive(&writer).result.is_ok());
}

#[test]
fn shutdown_drains_accepted_jobs_and_keeps_receipts_pollable() {
    let mut writer = CheckpointWriter::new(2);
    let completed = Arc::new(AtomicU64::new(0));
    for revision in 1..=2 {
        let completed = Arc::clone(&completed);
        writer
            .try_submit(key("shutdown"), revision, vec![], move |_| {
                completed.fetch_add(1, Ordering::AcqRel);
                Ok(())
            })
            .unwrap();
    }
    writer.shutdown().unwrap();
    assert_eq!(completed.load(Ordering::Acquire), 2);
    assert!(writer.try_recv().unwrap().result.is_ok());
    assert!(writer.try_recv().unwrap().result.is_ok());
    assert_eq!(writer.try_recv().unwrap_err(), TryRecvError::Disconnected);
}

#[test]
fn drop_drains_accepted_work_without_blocking_on_full_receipt_channel() {
    let (completed_tx, completed_rx) = mpsc::channel();
    {
        let writer = CheckpointWriter::new(2);
        for revision in 1..=2 {
            let completed_tx = completed_tx.clone();
            writer
                .try_submit(key("drop"), revision, vec![], move |_| {
                    completed_tx.send(revision).unwrap();
                    Ok(())
                })
                .unwrap();
        }
    }

    let mut completed = vec![completed_rx.recv().unwrap(), completed_rx.recv().unwrap()];
    completed.sort_unstable();
    assert_eq!(completed, [1, 2]);
}

#[test]
fn zero_capacity_still_has_a_finite_single_slot() {
    let writer = CheckpointWriter::new(0);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    writer
        .try_submit(key("zero"), 1, vec![], move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        writer.try_submit(key("zero"), 2, vec![], |_| Ok(())),
        Err(CheckpointSubmitError::Full)
    );
    release_tx.send(()).unwrap();
    assert!(receive(&writer).result.is_ok());
}

#[test]
fn independent_checkpoint_keys_progress_while_another_shard_is_blocked() {
    let writer = CheckpointWriter::new_with_workers(4, 2);
    let blocked_key = key_on_shard(0, 2);
    let free_key = key_on_shard(1, 2);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    writer
        .try_submit(blocked_key.clone(), 1, vec![], move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    writer
        .try_submit(free_key.clone(), 2, vec![], |_| Ok(()))
        .unwrap();
    let first = receive(&writer);
    assert_eq!(first.key, free_key);
    assert!(first.result.is_ok());
    release_tx.send(()).unwrap();
    let second = receive(&writer);
    assert_eq!(second.key, blocked_key);
    assert!(second.result.is_ok());
}

#[test]
fn same_key_keeps_submission_order_with_parallel_checkpoint_workers() {
    let writer = CheckpointWriter::new_with_workers(4, 2);
    let target = key_on_shard(0, 2);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    writer
        .try_submit(target.clone(), 1, vec![], move |_| {
            started_tx.send(1).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(started_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
    let (second_started_tx, second_started_rx) = mpsc::channel();
    writer
        .try_submit(target, 2, vec![], move |_| {
            second_started_tx.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    assert!(
        second_started_rx
            .recv_timeout(Duration::from_millis(20))
            .is_err()
    );
    release_tx.send(()).unwrap();
    assert_eq!(receive(&writer).revision, 1);
    assert_eq!(receive(&writer).revision, 2);
}

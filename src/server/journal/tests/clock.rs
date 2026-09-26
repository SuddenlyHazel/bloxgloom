use super::*;

#[test]
fn shared_clock_survives_tail_recovery_rotation_and_lower_tick_records() {
    let dir = TestDir::new();
    let path = dir.file();
    let mut journal = Journal::open(&path).unwrap();
    let mut first = state_tx(1, b"", b"first");
    first.tick = 40_000;
    append_direct(&mut journal, first).unwrap();
    assert_eq!(journal.max_tick(), 40_000);
    drop(journal);
    let mut journal = Journal::open(&path).unwrap();
    assert_eq!(journal.max_tick(), 40_000);
    let values = journal.latest_values();
    journal.rotate(journal.sequence()).unwrap();
    assert_eq!(
        journal.latest_values(),
        values,
        "clock is metadata, not a gameplay conflict key"
    );
    drop(journal);
    let mut journal = Journal::open(&path).unwrap();
    assert!(journal.records().is_empty());
    assert_eq!(journal.max_tick(), 40_000);
    assert_eq!(journal.latest_values(), values);
    append_direct(&mut journal, state_tx(2, b"first", b"second")).unwrap();
    assert_eq!(journal.max_tick(), 40_000);
    journal.rotate(journal.sequence()).unwrap();
    drop(journal);
    assert_eq!(Journal::open(&path).unwrap().max_tick(), 40_000);
}

#[test]
fn incomplete_or_rejected_records_cannot_advance_the_recovered_clock() {
    use std::io::Write;
    let dir = TestDir::new();
    let path = dir.file();
    write_one_transaction(&path, state_tx(1, b"", b"first"));
    let mut rejected = state_tx(2, b"wrong preimage", b"second");
    rejected.tick = 90_000;
    let mut journal = Journal::open(&path).unwrap();
    assert!(append_direct(&mut journal, rejected).is_err());
    assert_eq!(journal.max_tick(), 1);
    assert!(
        append_direct(
            &mut journal,
            Transaction::new(
                3,
                90_000,
                vec![Change::new(clock_key(), vec![], 90_000u64.to_le_bytes())]
            )
        )
        .is_err()
    );
    drop(journal);
    let mut partial = state_tx(4, b"first", b"second");
    partial.tick = 90_000;
    let frame = committed_frame(&partial);
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&frame[..frame.len() - 1])
        .unwrap();
    assert_eq!(Journal::open(&path).unwrap().max_tick(), 1);
}

use super::*;
use crate::server::journal::{Change, Journal, StateKey, Transaction};
use std::{collections::BTreeMap, fs, time::Duration};

#[test]
fn recovered_journal_noop_keeps_drop_revision_and_snapshot_unchanged() {
    let mut drops = Drops::new();
    drops.spawn([-2.0, 3.0, -4.0], 2, 9, Duration::from_millis(100));
    let revision = drops.revision();
    let snapshot = drops.snapshot_bytes().unwrap();
    let mut values = BTreeMap::new();
    values.insert(
        StateKey::new("bloxgloom:drop_owner", 1u64.to_le_bytes().to_vec()),
        drops.owner_snapshot(1),
    );
    values.insert(
        StateKey::new("bloxgloom:drop_allocator", Vec::new()),
        drops.allocator_snapshot(),
    );

    assert!(!drops.apply_recovered_journal(&values, false).unwrap());
    assert_eq!(drops.revision(), revision);
    assert_eq!(drops.snapshot_bytes().unwrap(), snapshot);
}

#[test]
fn closed_owner_set_rejects_unrepresented_snapshot_ids_but_legacy_mode_allows_them() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        7,
        [3.0, 4.0, 5.0],
        2,
        11,
        Duration::ZERO,
        Duration::ZERO,
    );
    let values = BTreeMap::new();

    // Old saves and legacy WALs have no closed-set proof, so retain their
    // existing permissive recovery behavior.
    assert!(drops.validate_recovered_journal(&values, false).is_ok());
    assert!(drops.validate_recovered_journal(&values, true).is_err());

    // A tail tombstone still represents the ID. A checkpoint that lags that
    // tail change is allowed by per-key WAL-chain validation in durable::open.
    let mut values = BTreeMap::new();
    values.insert(
        StateKey::new("bloxgloom:drop_owner", 7u64.to_le_bytes().to_vec()),
        Vec::new(),
    );
    values.insert(
        StateKey::new("bloxgloom:drop_allocator", Vec::new()),
        8u64.to_le_bytes().to_vec(),
    );
    assert!(drops.validate_recovered_journal(&values, true).is_ok());
}

#[test]
fn rotation_compaction_materializes_legacy_bgdp_drops_and_rejects_stale_ids() {
    let root = temp_root("bloxgloom-drop-owner-compaction");
    fs::create_dir_all(&root).unwrap();
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        7,
        [-4.5, 12.0, 1.25],
        2,
        23,
        Duration::ZERO,
        Duration::from_millis(100),
    );
    let legacy_owner = drops.owner_snapshot(7);
    let mut legacy_position = Vec::new();
    for coordinate in [-4.5f32, 12.0, 1.25] {
        legacy_position.extend(coordinate.to_le_bytes());
    }

    let path = root.join("server.wal");
    let writer = Journal::open(&path)
        .unwrap()
        .into_writer(8, Duration::ZERO)
        .unwrap();
    let spawn = Transaction::new(
        1,
        1,
        vec![
            Change::new(
                StateKey::new("bloxgloom:drop_owner", 1u64.to_le_bytes().to_vec()),
                Vec::new(),
                legacy_owner.clone(),
            ),
            Change::new(
                StateKey::new("bloxgloom:drop_position", 1u64.to_le_bytes().to_vec()),
                Vec::new(),
                legacy_position,
            ),
            Change::new(
                StateKey::new("bloxgloom:drop_allocator", Vec::new()),
                1u64.to_le_bytes().to_vec(),
                8u64.to_le_bytes().to_vec(),
            ),
        ],
    );
    writer
        .try_submit(spawn)
        .unwrap()
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    writer
        .try_submit(Transaction::new(
            2,
            2,
            vec![Change::new(
                StateKey::new("bloxgloom:drop_owner", 1u64.to_le_bytes().to_vec()),
                legacy_owner,
                Vec::new(),
            )],
        ))
        .unwrap()
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();

    // ID 7 came only from the legacy BGDP save. Rotation must include it in
    // the compacted base while discarding the historical ID 1 tombstone.
    let compacted = drops.rotation_compaction();
    let receipt = writer
        .try_rotate_with_drop_compaction(writer.sequence(), compacted)
        .unwrap()
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(receipt.cut_sequence, 2);
    writer.shutdown().unwrap();

    let journal = Journal::open(&path).unwrap();
    assert!(journal.drop_owner_set_closed());
    assert_eq!(journal.next_id().unwrap(), 3);
    let values = journal.latest_values();
    let id1_owner = StateKey::new("bloxgloom:drop_owner", 1u64.to_le_bytes().to_vec());
    let id1_position = StateKey::new("bloxgloom:drop_position", 1u64.to_le_bytes().to_vec());
    let id7_owner = StateKey::new("bloxgloom:drop_owner", 7u64.to_le_bytes().to_vec());
    assert!(!values.contains_key(&id1_owner));
    assert!(!values.contains_key(&id1_position));
    assert!(values.contains_key(&id7_owner));
    assert!(drops.validate_recovered_journal(&values, true).is_ok());
    assert!(!drops.apply_recovered_journal(&values, true).unwrap());

    // A checksum-valid pre-compaction copy containing a removed ID is not
    // silently accepted after the base declares the owner set closed.
    let mut stale_snapshot = Drops::new();
    insert_entry(
        &mut stale_snapshot,
        1,
        [0.0, 1.0, 0.0],
        2,
        3,
        Duration::ZERO,
        Duration::ZERO,
    );
    assert!(
        stale_snapshot
            .validate_recovered_journal(&values, true)
            .is_err()
    );
    fs::remove_dir_all(root).unwrap();
}

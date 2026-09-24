use super::*;
use std::fs;

fn drop_owner_key(id: u64) -> StateKey {
    StateKey::new("bloxgloom:drop_owner", id.to_le_bytes().to_vec())
}

fn drop_position_key(id: u64) -> StateKey {
    StateKey::new("bloxgloom:drop_position", id.to_le_bytes().to_vec())
}

fn drop_allocator_key() -> StateKey {
    StateKey::new("bloxgloom:drop_allocator", Vec::new())
}

fn drop_position_bytes(position: [f32; 3]) -> Vec<u8> {
    position.into_iter().flat_map(f32::to_le_bytes).collect()
}

fn drop_history_before_compaction(path: &PathBuf) -> Journal {
    let mut journal = Journal::open(path).unwrap();
    append_direct(
        &mut journal,
        Transaction::new(
            1,
            1,
            vec![
                Change::new(drop_owner_key(1), Vec::new(), vec![1, 2, 3]),
                Change::new(
                    drop_position_key(1),
                    Vec::new(),
                    drop_position_bytes([1.0, 2.0, 3.0]),
                ),
                Change::new(
                    drop_allocator_key(),
                    1u64.to_le_bytes().to_vec(),
                    3u64.to_le_bytes().to_vec(),
                ),
            ],
        ),
    )
    .unwrap();
    append_direct(
        &mut journal,
        Transaction::new(
            2,
            2,
            vec![Change::new(drop_owner_key(1), vec![1, 2, 3], Vec::new())],
        ),
    )
    .unwrap();
    journal
}

fn compacted_drop(id: u64, owner: Vec<u8>, position: [f32; 3]) -> CompactedDrop {
    CompactedDrop {
        id,
        owner,
        position,
    }
}

fn compaction_with_legacy_and_live_drops() -> DropCompaction {
    DropCompaction {
        // ID 9 represents an old BGDP-only drop that never had a WAL spawn.
        // ID 2 is a currently live WAL-owned drop.
        drops: vec![
            compacted_drop(2, vec![2, 2, 2], [2.0, 0.0, 0.0]),
            compacted_drop(9, vec![9, 9, 9], [9.0, 0.0, 0.0]),
        ],
        next_id: 10,
    }
}

#[test]
fn drop_compaction_prunes_tombstones_preserves_allocator_and_roundtrips_closed_set() {
    let dir = TestDir::new();
    let path = dir.file();
    let mut journal = drop_history_before_compaction(&path);
    let rotation = journal
        .rotate_with_drop_compaction(2, compaction_with_legacy_and_live_drops())
        .unwrap();
    assert_eq!(rotation.cut_sequence, 2);
    assert_eq!(journal.sequence(), 2);
    assert_eq!(journal.next_id().unwrap(), 3);
    assert!(journal.drop_owner_set_closed());
    let values = journal.latest_values();
    assert!(!values.contains_key(&drop_owner_key(1)));
    assert!(!values.contains_key(&drop_position_key(1)));
    assert_eq!(values[&drop_owner_key(2)], [2, 2, 2]);
    assert_eq!(
        values[&drop_position_key(2)],
        drop_position_bytes([2.0, 0.0, 0.0])
    );
    assert_eq!(values[&drop_owner_key(9)], [9, 9, 9]);
    assert_eq!(values[&drop_allocator_key()], 10u64.to_le_bytes());
    drop(journal);

    let reopened = Journal::open(path).unwrap();
    assert!(reopened.drop_owner_set_closed());
    assert_eq!(reopened.sequence(), 2);
    assert_eq!(reopened.next_id().unwrap(), 3);
    let values = reopened.latest_values();
    assert!(!values.contains_key(&drop_owner_key(1)));
    assert!(!values.contains_key(&drop_position_key(1)));
    assert_eq!(values[&drop_allocator_key()], 10u64.to_le_bytes());
}

#[test]
fn existing_v1_generation_base_opens_without_closed_owner_set_assumption() {
    let dir = TestDir::new();
    let path = dir.file();
    let mut journal = Journal::open(&path).unwrap();
    append_direct(&mut journal, state_tx(1, b"zero", b"one")).unwrap();
    journal.rotate(1).unwrap();
    drop(journal);

    // Convert the newly encoded base into the exact previous version-1
    // layout: remove the v2 marker byte, restore its format value, and refresh
    // the checksum. This exercises compatibility without a checked-in save.
    let manifest = rotation::read_manifest(&path).unwrap();
    let base_path = rotation::base_path(&path, &manifest).unwrap();
    let mut base = fs::read(&base_path).unwrap();
    base.remove(38);
    base[4..6].copy_from_slice(&1u16.to_le_bytes());
    let checksum_at = base.len() - 4;
    let checksum = crc32(&base[..checksum_at]);
    base[checksum_at..].copy_from_slice(&checksum.to_le_bytes());
    fs::write(base_path, base).unwrap();

    let reopened = Journal::open(&path).unwrap();
    assert!(!reopened.drop_owner_set_closed());
    assert_eq!(reopened.sequence(), 1);
    assert_eq!(
        reopened.latest_values()[&StateKey::new("bloxgloom:test_state", b"main".to_vec())],
        b"one"
    );
}

#[test]
fn compaction_crash_boundaries_select_only_the_old_or_closed_drop_set() {
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
        let mut journal = drop_history_before_compaction(&path);
        assert!(
            journal
                .rotate_with_drop_compaction_crashing_at(
                    2,
                    compaction_with_legacy_and_live_drops(),
                    point,
                )
                .is_err(),
            "{point:?} should simulate a crash"
        );
        drop(journal);

        let reopened = Journal::open(&path).unwrap();
        let values = reopened.latest_values();
        if point == rotation::CrashPoint::ManifestInstalled {
            assert!(reopened.drop_owner_set_closed(), "{point:?}");
            assert!(!values.contains_key(&drop_owner_key(1)), "{point:?}");
            assert!(!values.contains_key(&drop_position_key(1)), "{point:?}");
            assert_eq!(values[&drop_owner_key(9)], [9, 9, 9]);
            assert_eq!(reopened.sequence(), 2);
            assert_eq!(reopened.next_id().unwrap(), 3);
        } else {
            assert!(!reopened.drop_owner_set_closed(), "{point:?}");
            assert!(values.contains_key(&drop_owner_key(1)), "{point:?}");
            assert!(values.contains_key(&drop_position_key(1)), "{point:?}");
            assert_eq!(reopened.records().len(), 2);
        }
    }
}

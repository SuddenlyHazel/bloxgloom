use super::*;
use crate::server::parallel::OwnerKey;
use crate::server::registry::SystemId;
use crate::world::ChunkKey;

fn system(name: &str) -> SystemId {
    SystemId::new(name).unwrap()
}

#[test]
fn wake_keys_round_trip_for_all_owner_kinds() {
    let cases = [
        OwnerKey::Chunk(ChunkKey {
            x: -3,
            y: 7,
            z: 257,
        }),
        OwnerKey::Entity(9_999),
        OwnerKey::Profile(0x1234_5678_9abc_def0_1234_5678_9abc_def1),
    ];
    for owner in cases {
        let key = owner_wake_key(&system("test:owners"), owner);
        assert_eq!(key.domain, OWNER_WAKE_DOMAIN);
        assert_eq!(
            decode_owner_wake_key(&key),
            Some(("test:owners".into(), owner))
        );
    }
}

#[test]
fn malformed_wake_keys_decode_to_none() {
    let good = owner_wake_key(&system("test:owners"), OwnerKey::Entity(1));
    let mut truncated = good.clone();
    truncated.bytes.pop();
    assert_eq!(decode_owner_wake_key(&truncated), None);
    let mut bad_tag = good.clone();
    let tag_index = 2 + "test:owners".len();
    bad_tag.bytes[tag_index] = 9;
    assert_eq!(decode_owner_wake_key(&bad_tag), None);
    let foreign = StateKey::new("bloxgloom:owner_state", good.bytes.clone());
    assert_eq!(decode_owner_wake_key(&foreign), None);
}

#[test]
fn wake_values_round_trip_and_reject_tampering() {
    let encoded = encode_wake_value(41);
    assert_eq!(encoded.len(), OWNER_WAKE_VALUE_LEN);
    assert_eq!(decode_wake_value(&encoded).unwrap(), 41);
    let mut tampered = encoded.clone();
    tampered[7] ^= 0xff;
    assert_eq!(
        decode_wake_value(&tampered).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    let mut truncated = encoded.clone();
    truncated.truncate(8);
    assert_eq!(
        decode_wake_value(&truncated).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        decode_wake_value(&[]).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    let mut bad_magic = encoded.clone();
    bad_magic[0] ^= 0xff;
    // Fix the checksum so the magic check itself is what fails.
    let (body, _) = bad_magic.split_at(OWNER_WAKE_VALUE_LEN - 4);
    let crc = {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in body {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = crc & 1;
                crc >>= 1;
                if mask == 1 {
                    crc ^= 0xEDB8_8320;
                }
            }
        }
        !crc
    };
    bad_magic[OWNER_WAKE_VALUE_LEN - 4..].copy_from_slice(&crc.to_le_bytes());
    assert_eq!(
        decode_wake_value(&bad_magic).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn wake_value_length_is_fixed_and_tiny() {
    assert_eq!(OWNER_WAKE_VALUE_LEN, 4 + 2 + 8 + 4);
    assert_eq!(OWNER_WAKE_DOMAIN, "bloxgloom:owner_wake");
}

fn wake_dest(system: &str, owner: OwnerKey) -> (SystemId, OwnerKey) {
    (SystemId::new(system).unwrap(), owner)
}

#[test]
fn prepare_commit_serves_each_destination_once() {
    let mut store = PendingWakeStore::new();
    let dests = [
        wake_dest("test:wake_a", OwnerKey::Entity(1)),
        wake_dest("test:wake_a", OwnerKey::Entity(2)),
    ];
    let prepared = store.prepare_sets(&dests, 7, 8).unwrap();
    assert_eq!(prepared.changes_len(), 2);
    // Nothing is visible before the receipt.
    assert!(!store.contains(&dests[0].0, dests[0].1));
    assert_eq!(store.len(), 0);
    store.commit_sets(prepared);
    assert_eq!(store.len(), 2);
    assert!(store.contains(&dests[0].0, dests[0].1));
    // A duplicate wake for a flagged destination stages nothing: dedup is by
    // destination, so replaying the same intent cannot double-queue work.
    let repeat = store.prepare_sets(&dests, 9, 8).unwrap();
    assert_eq!(repeat.changes_len(), 0);
    store.commit_sets(repeat);
    assert_eq!(store.len(), 2);
    // Intra-wave duplicates collapse to one change per key: the journal
    // rejects duplicate keys within a single record.
    let doubled = [dests[0].clone(), dests[0].clone()];
    let mut fresh = PendingWakeStore::new();
    let collapsed = fresh.prepare_sets(&doubled, 7, 8).unwrap();
    assert_eq!(collapsed.changes_len(), 1);
}

#[test]
fn cancelled_sets_restage_on_retry() {
    let mut store = PendingWakeStore::new();
    let dests = [wake_dest("test:wake_a", OwnerKey::Entity(5))];
    let prepared = store.prepare_sets(&dests, 7, 8).unwrap();
    assert_eq!(prepared.changes_len(), 1);
    store.cancel_sets(prepared);
    assert_eq!(store.len(), 0);
    let retried = store.prepare_sets(&dests, 7, 8).unwrap();
    assert_eq!(retried.changes_len(), 1);
    store.commit_sets(retried);
    assert_eq!(store.len(), 1);
}

#[test]
fn wake_capacity_defers_without_stopping_the_coordinator() {
    let mut store = PendingWakeStore::new();
    let dests = [
        wake_dest("test:wake_a", OwnerKey::Entity(1)),
        wake_dest("test:wake_a", OwnerKey::Entity(2)),
    ];
    let error = store.prepare_sets(&dests, 7, 1).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_ne!(error.kind(), ErrorKind::InvalidData);
    // The rejected set staged nothing: a retry after budget frees succeeds.
    assert_eq!(store.len(), 0);
    let one = store.prepare_sets(&dests[..1], 7, 1).unwrap();
    assert_eq!(one.changes_len(), 1);
}

#[test]
fn wake_sets_recover_and_replay() {
    let dests = [
        wake_dest("test:wake_a", OwnerKey::Entity(1)),
        wake_dest("test:wake_b", OwnerKey::Entity(2)),
    ];
    // Recovery rebuilds the flagged set from latest-values, including the
    // producing tick envelope.
    let mut full_latest = BTreeMap::new();
    for ((system, owner), tick) in [dests[0].clone(), dests[1].clone()]
        .into_iter()
        .map(|(system, owner)| ((system, owner), 11))
        .chain([(
            (SystemId::new("test:wake_c").unwrap(), OwnerKey::Entity(3)),
            13,
        )])
    {
        full_latest.insert(owner_wake_key(&system, owner), encode_wake_value(tick));
    }
    // A cleared-flag tombstone (empty value) recovers as absent.
    full_latest.insert(
        owner_wake_key(&SystemId::new("test:wake_c").unwrap(), OwnerKey::Entity(99)),
        Vec::new(),
    );
    let recovered = PendingWakeStore::recover(&full_latest).unwrap();
    assert_eq!(recovered.len(), 3);
    assert!(recovered.contains(&dests[0].0, dests[0].1));

    // Receipted changes apply with before-value checks: sets land, clears
    // remove, and a stale before is corruption, not capacity.
    let mut live = PendingWakeStore::new();
    let set = crate::server::journal::Change::new(
        owner_wake_key(&dests[0].0, dests[0].1),
        Vec::new(),
        encode_wake_value(11),
    );
    live.apply_replayed(&[set]).unwrap();
    assert!(live.contains(&dests[0].0, dests[0].1));
    let clear = crate::server::journal::Change::new(
        owner_wake_key(&dests[0].0, dests[0].1),
        encode_wake_value(11),
        Vec::new(),
    );
    live.apply_replayed(&[clear]).unwrap();
    assert!(!live.contains(&dests[0].0, dests[0].1));
    // Foreign-domain keys are ignored.
    let foreign = crate::server::journal::Change::new(
        StateKey::new("bloxgloom:chunk_snapshot", vec![1, 2, 3]),
        Vec::new(),
        vec![9],
    );
    live.apply_replayed(&[foreign]).unwrap();
    // A before-value mismatch is genuine corruption and must stop the
    // coordinator, never defer it.
    let stale = crate::server::journal::Change::new(
        owner_wake_key(&dests[1].0, dests[1].1),
        encode_wake_value(999),
        encode_wake_value(11),
    );
    assert_eq!(
        live.apply_replayed(&[stale]).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn wake_recovery_rejects_corruption() {
    let mut bad_value = BTreeMap::new();
    bad_value.insert(
        owner_wake_key(&system("test:wake_a"), OwnerKey::Entity(1)),
        vec![0, 1, 2],
    );
    assert_eq!(
        PendingWakeStore::recover(&bad_value).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    let mut bad_key = BTreeMap::new();
    bad_key.insert(
        StateKey::new(OWNER_WAKE_DOMAIN, vec![0, 1, 2]),
        encode_wake_value(3),
    );
    assert_eq!(
        PendingWakeStore::recover(&bad_key).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

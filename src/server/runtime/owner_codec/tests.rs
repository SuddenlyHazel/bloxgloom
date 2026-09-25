use super::*;
use crate::server::parallel::OwnerKey;
use crate::server::registry::SystemId;
use crate::world::ChunkKey;

fn system(name: &str) -> SystemId {
    SystemId::new(name).unwrap()
}

#[test]
fn owner_keys_round_trip_for_all_owner_kinds() {
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
        let key = owner_state_key(&system("test:owners"), owner);
        assert_eq!(key.domain, OWNER_STATE_DOMAIN);
        assert_eq!(
            decode_owner_state_key(&key),
            Some(("test:owners".into(), owner))
        );
    }
}

#[test]
fn malformed_owner_keys_decode_to_none() {
    let good = owner_state_key(&system("test:owners"), OwnerKey::Entity(1));
    let mut truncated = good.clone();
    truncated.bytes.pop();
    assert_eq!(decode_owner_state_key(&truncated), None);
    let mut bad_tag = good.clone();
    let tag_index = 2 + "test:owners".len();
    bad_tag.bytes[tag_index] = 9;
    assert_eq!(decode_owner_state_key(&bad_tag), None);
    let foreign = StateKey::new("bloxgloom:chunk_snapshot", good.bytes.clone());
    assert_eq!(decode_owner_state_key(&foreign), None);
}

#[test]
fn cell_values_round_trip_and_reject_tampering() {
    let encoded = encode_cell_value(41, 3, Some(77), &[1, 2, 3, 4]);
    assert_eq!(
        decode_cell_value(&encoded).unwrap(),
        (41, 3, Some(77), vec![1, 2, 3, 4])
    );
    let undated = encode_cell_value(41, 3, None, &[1, 2, 3, 4]);
    assert_eq!(
        decode_cell_value(&undated).unwrap(),
        (41, 3, None, vec![1, 2, 3, 4])
    );
    let mut tampered = encoded.clone();
    tampered[25] ^= 0xff;
    assert_eq!(
        decode_cell_value(&tampered).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    let mut truncated = encoded.clone();
    truncated.truncate(8);
    assert_eq!(
        decode_cell_value(&truncated).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn owner_value_bound_mirrors_entity_payloads() {
    assert_eq!(MAX_OWNER_VALUE_BYTES, 64 * 1024);
}

#[test]
fn cursor_keys_round_trip() {
    let key = owner_cursor_key(&system("test:owners"));
    assert_eq!(key.domain, OWNER_CURSOR_DOMAIN);
    assert_eq!(decode_owner_cursor_key(&key), Some("test:owners".into()));
    let mut trailing = key.clone();
    trailing.bytes.push(0);
    assert_eq!(decode_owner_cursor_key(&trailing), None);
    let foreign = StateKey::new(OWNER_STATE_DOMAIN, key.bytes.clone());
    assert_eq!(decode_owner_cursor_key(&foreign), None);
}

#[test]
fn cursor_values_round_trip_for_all_owner_kinds() {
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
        let encoded = encode_cursor_value(owner);
        assert_eq!(encoded.len(), OWNER_CURSOR_VALUE_LEN);
        assert_eq!(decode_cursor_value(&encoded).unwrap(), owner);
    }
}

#[test]
fn cursor_values_reject_tampering() {
    let encoded = encode_cursor_value(OwnerKey::Entity(41));
    let mut tampered = encoded.clone();
    tampered[7] ^= 0xff;
    assert_eq!(
        decode_cursor_value(&tampered).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    let mut bad_tag = encoded.clone();
    bad_tag[6] = 9;
    let (body, _) = bad_tag.split_at(OWNER_CURSOR_VALUE_LEN - 4);
    let mut fixed = body.to_vec();
    let crc = {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in &fixed {
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
    fixed.extend(crc.to_le_bytes());
    assert_eq!(
        decode_cursor_value(&fixed).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        decode_cursor_value(&[]).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

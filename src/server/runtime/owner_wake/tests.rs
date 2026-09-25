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

use super::*;

#[test]
fn independent_container_codec_roundtrips_more_than_backpack_and_rejects_noncanonical_data() {
    let catalog = Catalog::builtins();
    let stack =
        Stack::with_components(crate::items::STICK, 128, 1, vec![7; MAX_COMPONENT_BYTES]).unwrap();
    let slots = vec![Some(stack); MAX_SLOTS];
    let bytes = encode(&slots, &catalog).unwrap();
    assert_eq!(bytes.len(), max_bytes(MAX_SLOTS));
    assert!(bytes.len() < 64 * 1024);
    assert_eq!(decode(&bytes, MAX_SLOTS, &catalog).unwrap(), slots);
    assert!(decode(&bytes, 36, &catalog).is_err());
    assert!(decode(&bytes[..bytes.len() - 1], MAX_SLOTS, &catalog).is_err());
    let mut invalid = bytes.clone();
    invalid.extend([0]);
    assert!(decode(&invalid, MAX_SLOTS, &catalog).is_err());
    let mut empty = encode(&[None], &catalog).unwrap();
    empty[12] = 1;
    assert!(decode(&empty, 1, &catalog).is_err());
    assert!(encode(&vec![None; MAX_SLOTS + 1], &catalog).is_err());
}

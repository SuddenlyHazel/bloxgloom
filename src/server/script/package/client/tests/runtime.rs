use super::*;

// Full artifact framing, not a decoder-only helper. Offsets point at the u32
// values so malformed counts/tags can be tested with a freshly valid SHA digest.
fn metadata() -> (Vec<u8>, BTreeMap<&'static str, usize>) {
    let mut w = header(1);
    let mut offsets = BTreeMap::new();
    package(&mut w, "demo", None);
    w.count(1).unwrap();
    w.count(3).unwrap();
    for cap in [
        "bloxgloom:actions/v1",
        "bloxgloom:generation/v1",
        "bloxgloom:owner_systems/v1",
    ] {
        w.field(cap.as_bytes()).unwrap();
    }
    w.count(0).unwrap(); // items
    w.count(0).unwrap(); // startup textures
    offsets.insert("actions", w.0.len());
    w.count(1).unwrap();
    w.field(b"demo:use").unwrap();
    offsets.insert("action_version", w.0.len());
    w.count(1).unwrap();
    w.field(b"Use").unwrap();
    offsets.insert("target", w.0.len());
    w.count(0).unwrap(); // empty
    offsets.insert("entities", w.0.len());
    w.count(1).unwrap();
    w.field(b"demo:counter").unwrap();
    offsets.insert("schema_version", w.0.len());
    w.count(1).unwrap();
    w.put(&17u64.to_le_bytes()).unwrap();
    offsets.insert("state_bytes", w.0.len());
    w.count(8).unwrap();
    offsets.insert("delay", w.0.len());
    w.count(0).unwrap();
    offsets.insert("handlers", w.0.len());
    w.count(1).unwrap();
    w.field(b"demo:use").unwrap();
    w.put(&18u64.to_le_bytes()).unwrap();
    offsets.insert("systems", w.0.len());
    w.count(1).unwrap();
    w.field(b"demo:clock").unwrap();
    w.put(&19u64.to_le_bytes()).unwrap();
    offsets.insert("generation", w.0.len());
    w.count(1).unwrap();
    w.field(b"demo:terrain").unwrap();
    offsets.insert("revision", w.0.len());
    w.count(1).unwrap();
    (w.0, offsets)
}

#[test]
fn runtime_metadata_rejects_unknown_shapes_and_unbounded_counts_with_valid_digest() {
    let (bytes, offsets) = metadata();
    let bundle = ClientBundle::decode_verify(&bytes, key(&bytes)).unwrap();
    let catalog = bundle.session_catalog().unwrap();
    assert!(catalog.action("demo:use").is_some());
    assert!(catalog.entity_type_id_by_key("demo:counter").is_some());
    assert!(catalog.gameplay_entity("demo:counter").is_none());
    for (field, value) in [
        ("actions", 2),
        ("action_version", 0),
        ("action_version", 65536),
        ("target", 3),
        ("entities", 33),
        ("schema_version", 0),
        ("state_bytes", 0),
        ("state_bytes", 65536),
        ("delay", 100001),
        ("handlers", 34),
        ("systems", 2),
        ("generation", 2),
        ("revision", 0),
        ("entities", u32::MAX),
    ] {
        let mut bad = bytes.clone();
        let offset = offsets[field];
        bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(
            ClientBundle::decode_verify(&bad, key(&bad)).is_err(),
            "{field}={value}"
        );
    }
    // Every structural truncation is rejected even if the sender recomputes SHA.
    for end in 0..bytes.len() {
        assert!(ClientBundle::decode_verify(&bytes[..end], key(&bytes[..end])).is_err());
    }
    for (start, end) in [("entities", "handlers"), ("handlers", "systems")] {
        let mut duplicate = bytes.clone();
        let start = offsets[start];
        let end = offsets[end];
        let record = duplicate[start + 4..end].to_vec();
        duplicate[start..start + 4].copy_from_slice(&2u32.to_le_bytes());
        duplicate.splice(end..end, record);
        assert!(ClientBundle::decode_verify(&duplicate, key(&duplicate)).is_err());
    }
    let mut old_format = bytes.clone();
    old_format[8] = 2;
    assert!(ClientBundle::decode_verify(&old_format, key(&old_format)).is_err());
    // Changing a server runtime key to another package cannot acquire identity.
    for key_name in [
        b"demo:use".as_slice(),
        b"demo:counter",
        b"demo:clock",
        b"demo:terrain",
    ] {
        let mut bad = bytes.clone();
        let offset = bad
            .windows(key_name.len())
            .position(|w| w == key_name)
            .unwrap();
        bad[offset] = b'x';
        assert!(ClientBundle::decode_verify(&bad, key(&bad)).is_err());
    }
    // Capabilities remain declarations, not client permissions; they must still
    // account for every category. A valid alternative requirement cannot replace
    // the one required by the corresponding metadata.
    let mut bad = bytes;
    let cap = b"bloxgloom:actions/v1";
    let offset = bad.windows(cap.len()).position(|w| w == cap).unwrap();
    bad[offset..offset + cap.len()].copy_from_slice(b"bloxgloom:content/v1");
    assert!(ClientBundle::decode_verify(&bad, key(&bad)).is_err());
}

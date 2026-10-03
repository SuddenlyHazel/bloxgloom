use super::*;

fn identity(entries: &[(&str, u32, Option<[u8; 32]>)]) -> Vec<u8> {
    let mut bytes = (entries.len() as u16).to_le_bytes().to_vec();
    for (key, revision, source) in entries {
        bytes.push(key.len() as u8);
        bytes.extend(key.as_bytes());
        bytes.extend(revision.to_le_bytes());
        match source {
            Some(source) => {
                bytes.push(1);
                bytes.extend(source);
            }
            None => bytes.push(0),
        }
    }
    bytes
}

fn metadata(identity: &[u8]) -> Vec<u8> {
    let mut bytes = WORLD_MAGIC.to_vec();
    bytes.extend(SAVE_FORMAT_VERSION.to_le_bytes());
    bytes.extend(TERRAIN_GENERATOR_VERSION.to_le_bytes());
    bytes.extend(42u64.to_le_bytes());
    bytes.extend(identity);
    bytes
}

#[test]
fn generation_diagnostics_identify_revision_source_removed_and_added_contracts() {
    let root = super::super::tests::temporary_root("metadata-contract");
    let path = root.join("world.meta");
    let original = identity(&[("sample:terrain", 1, Some([0x12; 32]))]);
    let before = metadata(&original);
    std::fs::write(&path, &before).unwrap();
    assert_eq!(read_world_metadata(&path, &original).unwrap(), 42);
    for (current, expected) in [
        (
            identity(&[("sample:terrain", 2, Some([0x12; 32]))]),
            "declared revision changed: saved 1, current 2",
        ),
        (
            identity(&[("sample:terrain", 1, Some([0x34; 32]))]),
            "source/dependency fingerprint changed at declared revision 1",
        ),
        (
            identity(&[]),
            "saved contributor is missing from the current installation",
        ),
        (
            identity(&[
                ("other:new", 1, None),
                ("sample:terrain", 1, Some([0x12; 32])),
            ]),
            "contributor added to the current installation, absent from save",
        ),
    ] {
        let error = read_world_metadata(&path, &current).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let text = error.to_string();
        assert!(text.contains(expected), "{text}");
        assert!(
            text.contains("namespace 'sample'") || text.contains("namespace 'other'"),
            "{text}"
        );
        assert!(
            text.contains("new world directory") && text.contains("existing save left unchanged"),
            "{text}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let changed = identity(&[("sample:terrain", 1, Some([0x34; 32]))]);
    let error = read_world_metadata(&path, &changed)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(&format!("saved sha256:{}", "12".repeat(32))),
        "{error}"
    );
    assert!(
        error.contains(&format!("current sha256:{}", "34".repeat(32))),
        "{error}"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_generation_identity_is_bounded_and_rejected_before_comparison() {
    let valid = identity(&[("sample:terrain", 1, Some([1; 32]))]);
    for end in 0..valid.len() {
        assert!(decode_identity(&valid[..end]).is_err());
    }
    let mut trailing = valid.clone();
    trailing.push(0);
    let mut unknown_tag = valid.clone();
    unknown_tag[2 + 1 + "sample:terrain".len() + 4] = 2;
    for malformed in [
        trailing,
        unknown_tag,
        u16::MAX.to_le_bytes().to_vec(),
        identity(&[("sample:duplicate", 1, None), ("sample:duplicate", 1, None)]),
        identity(&[("z:last", 1, None), ("a:first", 1, None)]),
        identity(&[("invalid", 1, None)]),
        identity(&[("sample:terrain", 0, None)]),
        vec![0; MAX_GENERATION_IDENTITY_BYTES + 1],
    ] {
        assert!(decode_identity(&malformed).is_err());
    }
    assert!(decode_identity(&valid).is_ok());
    assert!(decode_identity(&[0, 0]).is_ok());
}

#[test]
fn metadata_version_diagnostics_include_saved_and_current_without_writing() {
    let root = super::super::tests::temporary_root("metadata-version");
    let path = root.join("world.meta");
    let identity = identity(&[]);
    for (offset, saved, expected) in [
        (4, SAVE_FORMAT_VERSION - 1, "unsupported save format"),
        (
            6,
            TERRAIN_GENERATOR_VERSION - 1,
            "incompatible terrain generator 'bloxgloom:terrain'",
        ),
    ] {
        let mut bytes = metadata(&identity);
        bytes[offset..offset + 2].copy_from_slice(&saved.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        let error = read_world_metadata(&path, &identity)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(
            error.contains(&format!("saved {}", saved))
                || error.contains(&format!("saved revision {}", saved)),
            "{error}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    std::fs::remove_dir_all(root).unwrap();
}

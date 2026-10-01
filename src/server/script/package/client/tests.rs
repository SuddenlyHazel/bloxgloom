use super::*;
mod effects;
mod materials;
mod runtime;

#[test]
fn repeated_texture_bindings_cannot_multiply_one_asset_past_decoded_admission() {
    let mut writer = header(1);
    writer.field(b"farm").unwrap();
    writer.field(b"1.0.0").unwrap();
    writer.count(0).unwrap(); // dependencies
    writer.count(0).unwrap(); // sources
    writer.count(1).unwrap(); // one asset, reused by many declarations
    writer.field(b"tile").unwrap();
    writer.count(1).unwrap();
    let mut png = include_bytes!("../../../../../assets/textures/blocks/stone.png").to_vec();
    png.resize(MAX_ASSET_BYTES, 0);
    writer.field(&png).unwrap();
    writer.count(1).unwrap(); // startup
    writer.count(1).unwrap();
    writer.field(b"bloxgloom:content/v1").unwrap();
    writer.count(0).unwrap(); // items
    writer.count(33).unwrap();
    for index in 0..33 {
        writer
            .field(format!("farm:tile_{index:02}").as_bytes())
            .unwrap();
        writer.field(b"tile").unwrap();
    }
    writer.count(0).unwrap(); // blocks
    for _ in 0..5 {
        writer.count(0).unwrap();
    }
    let error = ClientBundle::decode_verify(&writer.0, key(&writer.0)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("estimated declaration bytes/installation"),
        "{error}"
    );
    // A failed verification releases its artifact admission and leaves the
    // next healthy artifact installable.
    let mut healthy = header(1);
    package(&mut healthy, "farm", None);
    healthy.count(0).unwrap();
    ClientBundle::decode_verify(&healthy.0, key(&healthy.0)).unwrap();
}

fn key(bytes: &[u8]) -> CacheKey {
    CacheKey::from_bytes(Sha256::digest(bytes).into())
}

fn header(count: usize) -> Writer {
    let mut writer = Writer(MAGIC.to_vec());
    writer.count(count).unwrap();
    writer
}

fn package(writer: &mut Writer, name: &str, dependency: Option<&str>) {
    writer.field(name.as_bytes()).unwrap();
    writer.field(b"1.0.0").unwrap();
    writer.count(usize::from(dependency.is_some())).unwrap();
    if let Some(dependency) = dependency {
        writer.field(dependency.as_bytes()).unwrap();
        writer.field(b"1.0.0").unwrap();
    }
    writer.count(0).unwrap();
    writer.count(0).unwrap();
}

#[test]
fn tampering_truncation_trailing_bytes_and_wrong_keys_are_rejected() {
    let mut writer = header(1);
    package(&mut writer, "app", None);
    writer.count(0).unwrap(); // no startup metadata
    let expected = key(&writer.0);
    ClientBundle::decode_verify(&writer.0, expected).unwrap();
    assert_eq!(expected.as_bytes(), &Sha256::digest(&writer.0)[..]);
    for len in 0..writer.0.len() {
        // Even if an attacker updates the expected hash, truncated structures fail.
        assert!(ClientBundle::decode_verify(&writer.0[..len], key(&writer.0[..len])).is_err());
    }
    let mut altered = writer.0.clone();
    *altered.last_mut().unwrap() ^= 1;
    assert!(ClientBundle::decode_verify(&altered, expected).is_err());
    assert!(ClientBundle::decode_verify(&writer.0, CacheKey::from_bytes([0; 32])).is_err());
    writer.0.push(0);
    assert!(ClientBundle::decode_verify(&writer.0, key(&writer.0)).is_err());
}

#[test]
fn canonical_order_dependency_identity_and_count_bounds_are_verified() {
    for (a, b, dependency) in [
        ("z", "a", None),
        ("a", "a", None),
        ("a", "b", Some("missing")),
        ("a", "b", Some("a")),
        ("../a", "b", None),
    ] {
        let mut writer = header(2);
        package(&mut writer, a, dependency);
        package(&mut writer, b, None);
        writer.count(0).unwrap();
        assert!(ClientBundle::decode_verify(&writer.0, key(&writer.0)).is_err());
    }
    let writer = header(MAX_PACKAGES + 1);
    assert!(ClientBundle::decode_verify(&writer.0, key(&writer.0)).is_err());
    let bytes = vec![0; MAX_BUNDLE_BYTES + 1];
    assert!(ClientBundle::decode_verify(&bytes, key(&bytes)).is_err());
    let mut writer = header(1);
    writer.count(u32::MAX as usize).unwrap(); // malicious string length
    assert!(ClientBundle::decode_verify(&writer.0, key(&writer.0)).is_err());
}

#[test]
fn decoder_rejects_server_classification_and_oversized_payloads_before_copying() {
    for (side, length) in [(0, 0), (3, 0), (1, MAX_SOURCE_BYTES + 1)] {
        let mut writer = header(1);
        writer.field(b"app").unwrap();
        writer.field(b"1.0.0").unwrap();
        writer.count(0).unwrap(); // dependencies
        writer.count(1).unwrap(); // sources
        writer.field(b"module").unwrap();
        writer.count(side).unwrap();
        writer.count(length).unwrap();
        writer.count(0).unwrap(); // assets
        assert!(ClientBundle::decode_verify(&writer.0, key(&writer.0)).is_err());
    }
}

fn sprite_metadata(owner: &str, requires: &[&str], items: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut writer = header(1);
    package(&mut writer, owner, None);
    writer.count(1).unwrap(); // startup metadata
    writer.count(requires.len()).unwrap();
    for requirement in requires {
        writer.field(requirement.as_bytes()).unwrap();
    }
    writer.count(items.len()).unwrap();
    for (key, display, texture) in items {
        for field in [key, display, texture] {
            writer.field(field.as_bytes()).unwrap();
        }
    }
    writer.count(0).unwrap(); // startup textures
    writer.count(0).unwrap(); // startup blocks
    for _ in 0..5 {
        writer.count(0).unwrap();
    } // runtime declaration categories
    writer.0
}

#[test]
fn sized_item_bundle_requires_canonical_version_and_valid_preset() {
    let mut writer = header(1);
    writer.0[MAGIC.len() - 1] = SIZED_MAGIC[MAGIC.len() - 1];
    package(&mut writer, "demo", None);
    writer.count(1).unwrap();
    writer.count(1).unwrap();
    writer.field(b"bloxgloom:content/v1").unwrap();
    writer.count(1).unwrap();
    for field in [b"demo:token".as_slice(), b"Token", b"bloxgloom:stone"] {
        writer.field(field).unwrap();
    }
    let size_offset = writer.0.len();
    writer.field(&[1]).unwrap();
    writer.count(0).unwrap(); // textures
    writer.count(0).unwrap(); // blocks
    for _ in 0..5 {
        writer.count(0).unwrap();
    }
    let verified = ClientBundle::decode_verify(&writer.0, key(&writer.0)).unwrap();
    let catalog = verified.session_catalog().unwrap();
    assert_eq!(
        catalog.drop_size(catalog.item_by_key("demo:token").unwrap()),
        bloxgloom_host_api::content::DropSize::Small
    );
    for value in [0, 3] {
        let mut altered = writer.0.clone();
        altered[size_offset + 4] = value;
        assert!(ClientBundle::decode_verify(&altered, key(&altered)).is_err());
    }
    let mut wrong_version = writer.0.clone();
    wrong_version[MAGIC.len() - 1] = MAGIC[MAGIC.len() - 1];
    assert!(ClientBundle::decode_verify(&wrong_version, key(&wrong_version)).is_err());
}

#[test]
fn animated_item_bundle_rejects_invalid_and_noncanonical_motion() {
    use bloxgloom_host_api::content::DropAnimation;
    let mut writer = header(1);
    writer.0[MAGIC.len() - 1] = ANIMATED_MAGIC[MAGIC.len() - 1];
    package(&mut writer, "demo", None);
    writer.count(1).unwrap();
    writer.count(1).unwrap();
    writer.field(b"bloxgloom:content/v1").unwrap();
    writer.count(1).unwrap();
    for field in [b"demo:token".as_slice(), b"Token", b"bloxgloom:stone"] {
        writer.field(field).unwrap();
    }
    writer.field(&[0]).unwrap();
    let motion_offset = writer.0.len() + 4;
    let animation = DropAnimation {
        pickup_duration: 0.8,
        ..Default::default()
    };
    writer.field(&animation.to_bytes()).unwrap();
    writer.count(0).unwrap();
    writer.count(0).unwrap();
    for _ in 0..5 {
        writer.count(0).unwrap();
    }
    let catalog = ClientBundle::decode_verify(&writer.0, key(&writer.0))
        .unwrap()
        .session_catalog()
        .unwrap();
    assert_eq!(
        catalog.drop_animation(catalog.item_by_key("demo:token").unwrap()),
        animation
    );
    for value in [f32::NAN, -1.0, 10.0, 0.34] {
        let mut invalid = writer.0.clone();
        invalid[motion_offset + 20..motion_offset + 24]
            .copy_from_slice(&value.to_bits().to_le_bytes());
        assert!(ClientBundle::decode_verify(&invalid, key(&invalid)).is_err());
    }
    let mut old_version = writer.0.clone();
    old_version[MAGIC.len() - 1] = SIZED_MAGIC[MAGIC.len() - 1];
    assert!(ClientBundle::decode_verify(&old_version, key(&old_version)).is_err());
}

#[test]
fn metadata_validates_namespace_capability_shape_and_limits_before_compilation() {
    const CONTENT: &str = bloxgloom_host_api::composition::CONTENT;
    let item = ("demo:token", "Token", "bloxgloom:stone");
    let bytes = sprite_metadata("demo", &[CONTENT], &[item]);
    let verified = ClientBundle::decode_verify(&bytes, key(&bytes)).unwrap();
    assert!(
        verified
            .session_catalog()
            .unwrap()
            .items()
            .any(|i| i.key == "demo:token")
    );
    for bytes in [
        sprite_metadata("demo", &[], &[item]),
        sprite_metadata("demo", &[CONTENT, CONTENT], &[item]),
        sprite_metadata("demo", &["bloxgloom:filesystem/v1"], &[item]),
        sprite_metadata(
            "bloxgloom",
            &[CONTENT],
            &[("bloxgloom:token", "Token", "bloxgloom:stone")],
        ),
        sprite_metadata(
            "demo",
            &[CONTENT],
            &[("other:token", "Token", "bloxgloom:stone")],
        ),
        sprite_metadata("demo", &[CONTENT], &[("demo:token", "", "bloxgloom:stone")]),
        sprite_metadata(
            "demo",
            &[CONTENT],
            &[("demo:token", &"x".repeat(256), "bloxgloom:stone")],
        ),
        sprite_metadata(
            "demo",
            &[CONTENT],
            &[("demo:token", "Token", "demo:custom")],
        ),
        sprite_metadata("demo", &[CONTENT], &[item, item]),
        sprite_metadata("demo", &[CONTENT], &[item; 33]),
    ] {
        assert!(ClientBundle::decode_verify(&bytes, key(&bytes)).is_err());
    }
    let bytes = sprite_metadata(
        "demo",
        &[CONTENT],
        &[("demo:token", "Token", "bloxgloom:missing")],
    );
    let bundle = ClientBundle::decode_verify(&bytes, key(&bytes)).unwrap();
    assert!(
        bundle
            .session_catalog()
            .unwrap_err()
            .to_string()
            .contains("missing texture")
    );
}

#[test]
fn policy_bundle_rejects_invalid_and_noncanonical_gameplay_policy() {
    use bloxgloom_host_api::content::{DropAnimation, DropPolicy};
    let mut writer = header(1);
    writer.0[MAGIC.len() - 1] = POLICY_MAGIC[MAGIC.len() - 1];
    package(&mut writer, "demo", None);
    writer.count(1).unwrap();
    writer.count(1).unwrap();
    writer.field(b"bloxgloom:content/v1").unwrap();
    writer.count(1).unwrap();
    for field in [b"demo:token".as_slice(), b"Token", b"bloxgloom:stone"] {
        writer.field(field).unwrap();
    }
    writer.field(&[0]).unwrap();
    writer.field(&DropAnimation::default().to_bytes()).unwrap();
    let offset = writer.0.len() + 4;
    let policy = DropPolicy {
        lifetime_ms: 2_000,
        ..Default::default()
    };
    writer.field(&policy.to_bytes()).unwrap();
    writer.count(0).unwrap();
    writer.count(0).unwrap();
    for _ in 0..5 {
        writer.count(0).unwrap();
    }
    let catalog = ClientBundle::decode_verify(&writer.0, key(&writer.0))
        .unwrap()
        .session_catalog()
        .unwrap();
    assert_eq!(
        catalog.drop_policy(catalog.item_by_key("demo:token").unwrap()),
        policy
    );
    for invalid in [
        DropPolicy::default(), // noncanonical V20 must not encode all-default policies
        DropPolicy {
            lifetime_ms: 0,
            ..policy
        },
        DropPolicy {
            radius: 10.0,
            ..policy
        },
        DropPolicy {
            gravity: f32::NAN,
            ..policy
        },
    ] {
        let mut bytes = writer.0.clone();
        bytes[offset..offset + DropPolicy::BYTE_LEN].copy_from_slice(&invalid.to_bytes());
        assert!(ClientBundle::decode_verify(&bytes, key(&bytes)).is_err());
    }
}

#[test]
fn startup_metadata_is_integrity_checked_and_cannot_be_truncated_even_with_new_digest() {
    let bytes = sprite_metadata(
        "demo",
        &[bloxgloom_host_api::composition::CONTENT],
        &[("demo:token", "Token", "bloxgloom:stone")],
    );
    let expected = key(&bytes);
    for len in 0..bytes.len() {
        assert!(ClientBundle::decode_verify(&bytes[..len], key(&bytes[..len])).is_err());
    }
    let mut changed = bytes;
    *changed.last_mut().unwrap() ^= 1;
    assert!(ClientBundle::decode_verify(&changed, expected).is_err());
}

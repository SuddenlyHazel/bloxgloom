use super::*;
use sha2::{Digest, Sha256};

struct Fixture(Cache);
impl Fixture {
    fn new() -> Self {
        Self(Cache {
            root: std::env::temp_dir().join(format!(
                "bloxgloom-bundle-disk-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            )),
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0.root);
    }
}

fn bundle(name: &str) -> (BundleIdentity, ClientBundle) {
    let mut bytes = b"BGCLIENT\x07".to_vec();
    bytes.extend(1u32.to_le_bytes());
    for field in [name.as_bytes(), b"1.0.0"] {
        bytes.extend((field.len() as u32).to_le_bytes());
        bytes.extend(field);
    }
    bytes.extend([0u8; 16]); // dependencies, sources, assets, startup metadata
    let key = crate::server::client_bundle::CacheKey::from_bytes(Sha256::digest(&bytes).into());
    let identity = BundleIdentity {
        client_runtime: protocol::CLIENT_RUNTIME_VERSION,
        key,
        total_len: bytes.len() as u32,
    };
    (identity, ClientBundle::decode_verify(&bytes, key).unwrap())
}

#[test]
fn disk_cache_survives_a_new_cache_instance_and_reverifies_corrupt_and_truncated_bytes() {
    let fixture = Fixture::new();
    let (identity, bundle) = bundle("demo");
    assert!(fixture.0.load(identity).unwrap().is_none());
    fixture.0.store(identity, &bundle).unwrap();
    let restarted = Cache {
        root: fixture.0.root.clone(),
    };
    let loaded = restarted.load(identity).unwrap().unwrap();
    assert_eq!(loaded.cache_key(), identity.key);
    assert_eq!(loaded.bytes(), bundle.bytes());
    let mut corrupt = bundle.bytes().to_vec();
    corrupt[0] ^= 1;
    fs::write(fixture.0.path(identity), corrupt).unwrap();
    assert!(restarted.load(identity).unwrap().is_none());
    assert!(!fixture.0.path(identity).exists());
    fs::write(fixture.0.path(identity), b"partial").unwrap();
    assert!(restarted.load(identity).unwrap().is_none());
    fixture.0.store(identity, &bundle).unwrap();
    let unsupported = BundleIdentity {
        client_runtime: identity.client_runtime + 1,
        ..identity
    };
    assert!(fixture.0.load(unsupported).is_err());
    assert!(fixture.0.store(unsupported, &bundle).is_err());
    assert!(fixture.0.path(identity).exists());
}

#[test]
fn disk_cache_caps_entries_bytes_and_recovers_interrupted_writes_without_touching_other_files() {
    let fixture = Fixture::new();
    for i in 0..=MAX_ENTRIES {
        let (id, b) = bundle(&format!("demo{i}"));
        fixture.0.store(id, &b).unwrap();
    }
    assert_eq!(
        fs::read_dir(&fixture.0.root)
            .unwrap()
            .filter(|e| cache_name(e.as_ref().unwrap().file_name().to_str().unwrap()))
            .count(),
        MAX_ENTRIES
    );
    fs::write(fixture.0.root.join("pending-crash.tmp"), b"partial").unwrap();
    fs::write(fixture.0.root.join("unrelated.txt"), b"keep").unwrap();
    let (identity, b) = bundle("newest");
    let (oversized, _) = bundle("oversized");
    let file = File::create(fixture.0.path(oversized)).unwrap();
    file.set_len(MAX_BYTES).unwrap();
    drop(file);
    fixture.0.store(identity, &b).unwrap();
    let total: u64 = fs::read_dir(&fixture.0.root)
        .unwrap()
        .filter_map(|e| {
            let e = e.unwrap();
            cache_name(e.file_name().to_str().unwrap()).then(|| e.metadata().unwrap().len())
        })
        .sum();
    assert!(total <= MAX_BYTES);
    assert!(!fixture.0.root.join("pending-crash.tmp").exists());
    assert_eq!(
        fs::read(fixture.0.root.join("unrelated.txt")).unwrap(),
        b"keep"
    );
    assert!(fixture.0.load(identity).unwrap().is_some());
}

#[test]
fn disk_cache_rejects_wrong_bundle_identity_and_skips_a_contended_writer_lock() {
    let fixture = Fixture::new();
    let (identity, b) = bundle("demo");
    let (different, _) = bundle("other");
    assert!(fixture.0.store(different, &b).is_err());
    fs::create_dir_all(&fixture.0.root).unwrap();
    let lock = File::create(fixture.0.root.join("writer.lock")).unwrap();
    lock.lock().unwrap();
    assert!(fixture.0.store(identity, &b).is_err());
    assert!(!fixture.0.path(identity).exists());
    drop(lock);
    fixture.0.store(identity, &b).unwrap();
    assert!(fixture.0.load(identity).unwrap().is_some());
}

use super::image::SourceImage;
use super::legacy::checksum;
use super::migrate_v4;
use crate::content::BlockStateId;
use crate::inventory::InventoryStore;
use crate::server::journal::{Change, Journal, StateKey, Transaction};
use crate::world::{ChunkKey, TERRAIN_GENERATOR_VERSION};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "bloxgloom-v4-convert-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn source(&self) -> PathBuf {
        self.0.join("old-world")
    }
    fn target(&self) -> PathBuf {
        self.0.join("new-world")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn old_edits(seed: u64, revision: u64, state: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(b"BGED");
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(TERRAIN_GENERATOR_VERSION.to_le_bytes());
    bytes.extend(seed.to_le_bytes());
    bytes.extend(revision.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(7u16.to_le_bytes());
    bytes.push(state);
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

fn old_inventory(revision: u64, count: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(b"BGIN");
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(revision.to_le_bytes());
    bytes.push(1);
    bytes.extend(count.to_le_bytes());
    bytes.resize(bytes.len() + 35 * 3, 0);
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

fn old_drops() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(b"BGDP");
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(3u64.to_le_bytes());
    bytes.extend(2u64.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(1u64.to_le_bytes());
    bytes.push(1);
    bytes.extend(3u16.to_le_bytes());
    for coordinate in [0.0f32, 88.0, 0.0] {
        bytes.extend(coordinate.to_le_bytes());
    }
    bytes.extend(1_700_000_000_000u64.to_le_bytes());
    bytes.extend(250u16.to_le_bytes());
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

fn old_content_map() -> Vec<u8> {
    let mut identities = crate::content::Catalog::builtins()
        .identities()
        .into_iter()
        .filter(|(kind, _, _, _)| matches!(kind, b'B' | b'I'))
        .map(|(kind, id, key, _)| (kind, id as u8, key.to_owned()))
        .collect::<Vec<_>>();
    identities.sort_by_key(|(kind, id, _)| (*kind, *id));
    let mut bytes = b"BGCM".to_vec();
    bytes.extend(1u16.to_le_bytes());
    bytes.extend((identities.len() as u16).to_le_bytes());
    for (kind, id, key) in identities {
        bytes.extend([kind, id, key.len() as u8]);
        bytes.extend(key.as_bytes());
    }
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

fn source_fixture(fixture: &Fixture) -> (Vec<u8>, Vec<u8>) {
    let source = fixture.source();
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("players")).unwrap();
    let seed = 73u64;
    let mut meta = Vec::new();
    meta.extend(b"BGWD");
    meta.extend(TERRAIN_GENERATOR_VERSION.to_le_bytes());
    meta.extend(seed.to_le_bytes());
    fs::write(source.join("world.meta"), meta).unwrap();
    fs::write(source.join("content.map"), old_content_map()).unwrap();
    let before_edit = old_edits(seed, 1, 1);
    let after_edit = old_edits(seed, 2, 2);
    fs::write(source.join("-1_0_2.bged"), &before_edit).unwrap();
    let before_inventory = old_inventory(1, 2);
    let after_inventory = old_inventory(2, 5);
    fs::write(
        source.join("players/0000000000000000000000000000002a.inv"),
        &before_inventory,
    )
    .unwrap();
    fs::write(source.join("drops.bin"), old_drops()).unwrap();

    let mut chunk_key = Vec::new();
    for coordinate in [-1i32, 0, 2] {
        chunk_key.extend(coordinate.to_le_bytes());
    }
    let mut receipt_key = Vec::new();
    receipt_key.extend(42u128.to_le_bytes());
    receipt_key.extend(99u128.to_le_bytes());
    let mut owner_before = vec![1, 1];
    owner_before.extend(3u16.to_le_bytes());
    owner_before.extend(1_700_000_000_000u64.to_le_bytes());
    owner_before.extend(250u16.to_le_bytes());
    let mut owner_after = vec![1, 1];
    owner_after.extend(2u16.to_le_bytes());
    owner_after.extend(1_700_000_000_000u64.to_le_bytes());
    owner_after.extend(250u16.to_le_bytes());
    let journal = Journal::open(source.join("server.wal")).unwrap();
    let writer = journal.into_writer(4, Duration::ZERO).unwrap();
    let tx = Transaction::new(
        1,
        1,
        vec![
            Change::new(
                StateKey::new("bloxgloom:chunk_snapshot", chunk_key),
                before_edit.clone(),
                after_edit.clone(),
            ),
            Change::new(
                StateKey::new("bloxgloom:inventory", 42u128.to_le_bytes().to_vec()),
                before_inventory,
                after_inventory,
            ),
            Change::new(
                StateKey::new("bloxgloom:drop_owner", 1u64.to_le_bytes().to_vec()),
                owner_before,
                owner_after,
            ),
            Change::new(
                StateKey::new("bloxgloom:action_receipt", receipt_key),
                Vec::new(),
                vec![1, 2, 0, 1, 0],
            ),
        ],
    );
    writer.try_submit(tx).unwrap().recv().unwrap().unwrap();
    drop(writer);
    (before_edit, after_edit)
}

#[test]
fn copied_v4_save_recovers_pending_wal_and_closes_legacy_actions() {
    let fixture = Fixture::new();
    let (old_file, _) = source_fixture(&fixture);
    let source = fixture.source();
    let target = fixture.target();
    let report = migrate_v4(&source, &target, true).unwrap();
    assert_eq!(report.edited_chunks, 1);
    assert_eq!(report.inventories, 1);
    assert_eq!(report.drops, 1);
    assert_eq!(report.legacy_action_receipts, 1);
    assert_eq!(fs::read(source.join("-1_0_2.bged")).unwrap(), old_file);
    assert!(!source.join(".world.lock").exists());
    assert!(target.join("conversion.complete").exists());
    assert!(!target.join(".conversion-incomplete").exists());

    let storage = crate::storage::Storage::new(&target, 73).unwrap();
    let edits = storage.load(ChunkKey { x: -1, y: 0, z: 2 }).unwrap();
    assert_eq!(edits.version, 2);
    assert_eq!(edits.blocks.get(&7), Some(&BlockStateId(2)));
    let inventory = InventoryStore::new(&target).unwrap().load(42).unwrap();
    assert_eq!(inventory.revision, 2);
    assert_eq!(inventory.slots[0].as_ref().unwrap().count, 5);
    let drops = crate::server::drops::Drops::open(&target).unwrap();
    assert_eq!(drops.stack(1).unwrap().count, 2);
    let journal = Journal::open(target.join("server.wal")).unwrap();
    assert!(journal.drop_owner_set_closed());
    assert_eq!(
        journal
            .latest_values()
            .keys()
            .filter(|key| key.domain == "bloxgloom:action_ledger")
            .count(),
        1
    );
    assert!(
        journal
            .latest_values()
            .keys()
            .all(|key| key.domain != "bloxgloom:action_receipt")
    );
    drop(journal);
    drop(storage);
    let state = crate::server::server_state(73, target).unwrap();
    drop(state);
}

#[test]
fn changed_source_is_rejected_and_missing_stopped_assertion_does_not_create_destination() {
    let fixture = Fixture::new();
    source_fixture(&fixture);
    assert!(migrate_v4(fixture.source(), fixture.target(), false).is_err());
    assert!(!fixture.target().exists());
    let image = SourceImage::capture(&fixture.source(), &fixture.target()).unwrap();
    fs::write(fixture.source().join("drops.bin"), b"modified").unwrap();
    assert!(image.verify_unchanged().is_err());
}

#[test]
fn corrupted_legacy_receipt_fails_before_target_publication() {
    let fixture = Fixture::new();
    source_fixture(&fixture);
    let wal = fixture.source().join("server.wal");
    let mut bytes = fs::read(&wal).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x80;
    fs::write(&wal, bytes).unwrap();
    assert!(migrate_v4(fixture.source(), fixture.target(), true).is_err());
    assert!(!fixture.target().exists());
}

#[test]
fn interrupted_conversion_stage_is_not_a_loadable_world() {
    let fixture = Fixture::new();
    source_fixture(&fixture);
    let drops_path = fixture.source().join("drops.bin");
    let mut corrupted = fs::read(&drops_path).unwrap();
    corrupted[10] ^= 0x40;
    fs::write(&drops_path, &corrupted).unwrap();

    assert!(migrate_v4(fixture.source(), fixture.target(), true).is_err());
    assert!(!fixture.target().exists());
    assert_eq!(fs::read(drops_path).unwrap(), corrupted);
    let partial = fs::read_dir(&fixture.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(".new-world.migration-v4.") && name.ends_with(".partial")
                })
        })
        .expect("failed conversion retains a partial stage for inspection");
    assert!(crate::storage::Storage::new(partial, 73).is_err());
}

#[test]
fn existing_destination_is_never_replaced() {
    let fixture = Fixture::new();
    source_fixture(&fixture);
    fs::create_dir(fixture.target()).unwrap();
    fs::write(fixture.target().join("owner-note"), b"preserve me").unwrap();
    assert!(migrate_v4(fixture.source(), fixture.target(), true).is_err());
    assert_eq!(
        fs::read(fixture.target().join("owner-note")).unwrap(),
        b"preserve me"
    );
}

//! Convert a recovered copy, validate all committed values, then publish it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::content::Catalog;
use crate::inventory::InventoryStore;
use crate::server::drops::Drops;
use crate::server::durable;
use crate::server::journal::{Change, Journal, StateKey, Transaction};
use crate::storage::{Storage, legacy::LegacyIdMap};
use crate::world::ChunkKey;

use super::MigrationReport;
use super::image::{SourceImage, parse_chunk, parse_profile};
use super::legacy::{
    LegacyDrops, convert_owner, default_inventory_bytes, validate_action_receipt, validate_position,
};

const MAX_OLD_EDITS: usize = 26 + 4096 * 3 + 4;
const MAX_OLD_INVENTORY: usize = 4 + 2 + 8 + 36 * 3 + 4;
const MAX_OLD_DROPS: usize = 26 + 1_000_000 * 33 + 4;
const MAX_OLD_CONTENT: usize = 8 + 512 * (3 + 255) + 4;

/// Convert a v4 world into a new v5 directory. `source_stopped` is an explicit
/// operator assertion: old binaries do not honor the converter's lock. The
/// source is only read; all journal recovery and tail truncation occur on its
/// private copy. A failed run leaves an inspectable, unstartable partial stage.
pub fn migrate_v4(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    source_stopped: bool,
) -> io::Result<MigrationReport> {
    if !source_stopped {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "stop the v4 server and pass --source-stopped",
        ));
    }
    let image = SourceImage::capture(source.as_ref(), destination.as_ref())?;
    let stage = image.stage()?;
    write_new(
        &stage.join(crate::storage::CONVERSION_INCOMPLETE),
        b"v4 to v5 conversion in progress\n",
    )?;
    let source_copy = image.copy_into(&stage)?;
    let seed = crate::storage::legacy::decode_world_meta_v4(&read_bounded(
        &source_copy.join("world.meta"),
        14,
    )?)?;
    let candidate = Arc::new(crate::content::catalog().clone());
    let legacy_content = read_optional_bounded(&source_copy.join("content.map"), MAX_OLD_CONTENT)?;
    let ids = LegacyIdMap::from_content_map_v1(legacy_content.as_deref(), &candidate)?;

    // The encoder owns a temporary v5 world header and catalog. The stage's
    // incomplete marker remains present throughout validation.
    let encoder_root = stage.join(".encoder");
    let encoder = Storage::with_catalog(&encoder_root, seed, Arc::clone(&candidate))?;
    let catalog = encoder.catalog_arc();
    let legacy_journal = Journal::open(source_copy.join("server.wal"))?;
    let latest = legacy_journal.latest_values();
    let legacy_drops = LegacyDrops::decode(
        read_optional_bounded(&source_copy.join("drops.bin"), MAX_OLD_DROPS)?.as_deref(),
        &ids,
    )?;
    let mut chunks = collect_chunks(&source_copy)?;
    let mut inventories = collect_inventories(&source_copy)?;
    let mut drop_values = BTreeMap::new();
    let mut receipt_profiles = BTreeSet::new();
    let mut legacy_action_receipts = 0usize;

    for (key, value) in &latest {
        match key.domain.as_str() {
            "bloxgloom:chunk_snapshot" => {
                let chunk = decode_chunk_key(&key.bytes)?;
                let current = chunks.get(&chunk).map_or(&[][..], Vec::as_slice);
                if !current.is_empty() {
                    crate::storage::legacy::decode_bged_v2(current, seed, &ids)?;
                }
                legacy_journal.validate_snapshot(key, current)?;
                if !value.is_empty() {
                    crate::storage::legacy::decode_bged_v2(value, seed, &ids)?;
                }
                chunks.insert(chunk, value.clone());
            }
            "bloxgloom:inventory" => {
                let profile = decode_profile_key(&key.bytes)?;
                let fallback = default_inventory_bytes();
                let current = inventories
                    .get(&profile)
                    .map_or(fallback.as_slice(), Vec::as_slice);
                InventoryStore::decode_legacy_snapshot(current, &ids)?;
                legacy_journal.validate_snapshot(key, current)?;
                InventoryStore::decode_legacy_snapshot(value, &ids)?;
                inventories.insert(profile, value.clone());
            }
            "bloxgloom:drop_owner" => {
                let id = decode_drop_id(&key.bytes)?;
                legacy_journal.validate_snapshot(key, &legacy_drops.old_owner_snapshot(id))?;
                drop_values.insert(key.clone(), convert_owner(id, value, &ids)?);
            }
            "bloxgloom:drop_allocator" => {
                if !key.bytes.is_empty()
                    || value.len() != 8
                    || u64::from_le_bytes(value[..8].try_into().unwrap()) == 0
                {
                    return Err(invalid("invalid legacy drop allocator WAL value"));
                }
                legacy_journal.validate_snapshot(key, &legacy_drops.next_id.to_le_bytes())?;
                drop_values.insert(key.clone(), value.clone());
            }
            "bloxgloom:drop_position" => {
                decode_drop_id(&key.bytes)?;
                if !value.is_empty() {
                    validate_position(value)?;
                }
                drop_values.insert(key.clone(), value.clone());
            }
            "bloxgloom:action_receipt" => {
                if key.bytes.len() != 32 {
                    return Err(invalid("invalid legacy action receipt key"));
                }
                let profile = decode_profile_key(&key.bytes[..16])?;
                let action = u128::from_le_bytes(key.bytes[16..32].try_into().unwrap());
                if action == 0 {
                    return Err(invalid("zero legacy action ID"));
                }
                validate_action_receipt(value, &ids)?;
                receipt_profiles.insert(profile);
                legacy_action_receipts = legacy_action_receipts
                    .checked_add(1)
                    .ok_or_else(|| invalid("legacy receipt count overflow"))?;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported legacy journal domain: {other}"),
                ));
            }
        }
    }

    let edited_chunks = install_chunks(&stage, &encoder, &chunks, seed, &ids)?;
    let inventories = install_inventories(&stage, &catalog, &inventories, &ids)?;
    let drops = install_drops(
        &stage,
        &catalog,
        &legacy_drops,
        &drop_values,
        legacy_journal.drop_owner_set_closed(),
    )?;
    let drop_count = drops.rotation_compaction().drops.len();
    install_ledgers_and_journal(&stage, &receipt_profiles, &drops)?;
    copy_new(&encoder_root.join("world.meta"), &stage.join("world.meta"))?;
    copy_new(
        &encoder_root.join("content.map"),
        &stage.join("content.map"),
    )?;
    validate_stage(
        &stage,
        &encoder,
        &catalog,
        &chunks,
        &inventories,
        &receipt_profiles,
        drop_count,
    )?;

    // The raw v4 copy and temporary encoder are owned by this stage. They are
    // removed only after the complete v5 state has passed validation.
    fs::remove_dir_all(&source_copy)?;
    drop(encoder);
    fs::remove_dir_all(&encoder_root)?;
    File::open(&stage)?.sync_all()?;
    image.verify_unchanged()?;
    write_new(
        &stage.join("conversion.complete"),
        b"v4 source validated; legacy action namespace closed\n",
    )?;
    fs::remove_file(stage.join(crate::storage::CONVERSION_INCOMPLETE))?;
    File::open(&stage)?.sync_all()?;
    image.verify_unchanged()?;
    let destination = image.destination();
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination appeared during conversion",
        ));
    }
    rename_new(&stage, destination)?;
    File::open(destination.parent().expect("absolute destination"))?.sync_all()?;
    Ok(MigrationReport {
        source_files: image.source_file_count(),
        edited_chunks,
        inventories,
        drops: drop_count,
        legacy_action_receipts,
    })
}

fn collect_chunks(root: &Path) -> io::Result<BTreeMap<ChunkKey, Vec<u8>>> {
    let mut chunks = BTreeMap::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(key) = name.to_str().and_then(parse_chunk) else {
            continue;
        };
        if chunks
            .insert(key, read_bounded(&entry.path(), MAX_OLD_EDITS)?)
            .is_some()
        {
            return Err(invalid("duplicate legacy chunk key"));
        }
    }
    Ok(chunks)
}

fn collect_inventories(root: &Path) -> io::Result<BTreeMap<u128, Vec<u8>>> {
    let mut inventories = BTreeMap::new();
    let players = root.join("players");
    if !players.exists() {
        return Ok(inventories);
    }
    for entry in fs::read_dir(players)? {
        let entry = entry?;
        let name = entry.file_name();
        let profile = name
            .to_str()
            .and_then(parse_profile)
            .ok_or_else(|| invalid("invalid legacy inventory filename"))?;
        if inventories
            .insert(profile, read_bounded(&entry.path(), MAX_OLD_INVENTORY)?)
            .is_some()
        {
            return Err(invalid("duplicate legacy profile ID"));
        }
    }
    Ok(inventories)
}

fn install_chunks(
    stage: &Path,
    encoder: &Storage,
    chunks: &BTreeMap<ChunkKey, Vec<u8>>,
    seed: u64,
    ids: &LegacyIdMap,
) -> io::Result<usize> {
    let mut count = 0;
    for (key, old) in chunks {
        if old.is_empty() {
            continue;
        }
        let edits = crate::storage::legacy::decode_bged_v2(old, seed, ids)?;
        let bytes = encoder
            .encode_snapshot(&edits)?
            .ok_or_else(|| invalid("nonempty legacy edit became pristine"))?;
        encoder.decode_snapshot(Some(&bytes))?;
        write_new(
            &stage.join(format!("{}_{}_{}.bged", key.x, key.y, key.z)),
            &bytes,
        )?;
        count += 1;
    }
    Ok(count)
}

fn install_inventories(
    stage: &Path,
    catalog: &Arc<Catalog>,
    inventories: &BTreeMap<u128, Vec<u8>>,
    ids: &LegacyIdMap,
) -> io::Result<usize> {
    let store = InventoryStore::with_catalog(stage, Arc::clone(catalog))?;
    for (&profile, old) in inventories {
        let inventory = InventoryStore::decode_legacy_snapshot(old, ids)?;
        let bytes = InventoryStore::encode_snapshot_with_catalog(&inventory, catalog)?;
        store.checkpoint_snapshot(profile, &bytes)?;
        if store.load(profile)? != inventory {
            return Err(invalid("converted inventory roundtrip mismatch"));
        }
    }
    Ok(inventories.len())
}

fn install_drops(
    stage: &Path,
    catalog: &Arc<Catalog>,
    old: &LegacyDrops,
    values: &BTreeMap<StateKey, Vec<u8>>,
    closed: bool,
) -> io::Result<Drops> {
    write_new(&stage.join("drops.bin"), &old.encode_v5_checkpoint())?;
    let mut drops = Drops::open_with_catalog(stage, Arc::clone(catalog))?;
    drops.validate_recovered_journal(values, closed)?;
    drops.apply_recovered_journal(values, closed)?;
    drops.save()?;
    let reopened = Drops::open_with_catalog(stage, Arc::clone(catalog))?;
    let original = drops.rotation_compaction();
    let restored = reopened.rotation_compaction();
    let equal_entries = original.drops.len() == restored.drops.len()
        && original
            .drops
            .iter()
            .zip(&restored.drops)
            .all(|(left, right)| {
                left.id == right.id && left.owner == right.owner && left.position == right.position
            });
    if !equal_entries || original.next_id != restored.next_id {
        return Err(invalid("converted drops roundtrip mismatch"));
    }
    Ok(drops)
}

fn install_ledgers_and_journal(
    stage: &Path,
    receipt_profiles: &BTreeSet<u128>,
    drops: &Drops,
) -> io::Result<()> {
    let receipt_dir = stage.join("receipts");
    fs::create_dir(&receipt_dir)?;
    let ledger = durable::closed_legacy_ledger_bytes()?;
    let journal = Journal::open(stage.join("server.wal"))?;
    let writer = journal.into_writer(32, Duration::ZERO)?;
    let mut next_id = 1u128;
    for &profile in receipt_profiles {
        let path = receipt_dir.join(format!("{profile:032x}.ledger"));
        write_new(&path, &ledger)?;
        let tx = Transaction::new(
            next_id,
            0,
            vec![Change::new(
                StateKey::new("bloxgloom:action_ledger", profile.to_le_bytes().to_vec()),
                Vec::new(),
                ledger.clone(),
            )],
        );
        let receipt = writer
            .try_submit(tx)
            .map_err(|error| io::Error::other(format!("converted ledger WAL submission: {error}")))?
            .recv()
            .map_err(|_| io::Error::other("converted ledger WAL worker stopped"))??;
        if receipt.id != next_id || receipt.duplicate {
            return Err(invalid("converted ledger WAL receipt mismatch"));
        }
        next_id = next_id
            .checked_add(1)
            .ok_or_else(|| invalid("converted transaction ID overflow"))?;
    }
    File::open(&receipt_dir)?.sync_all()?;
    let cut = writer.sequence();
    let receipt = writer
        .try_rotate_with_drop_compaction(cut, drops.rotation_compaction())
        .map_err(|error| io::Error::other(format!("converted WAL rotation: {error}")))?
        .recv()
        .map_err(|_| io::Error::other("converted WAL rotation worker stopped"))??;
    if receipt.cut_sequence != cut {
        return Err(invalid("converted WAL cut mismatch"));
    }
    drop(writer);
    Ok(())
}

fn validate_stage(
    stage: &Path,
    storage: &Storage,
    catalog: &Arc<Catalog>,
    chunks: &BTreeMap<ChunkKey, Vec<u8>>,
    inventories: &usize,
    receipt_profiles: &BTreeSet<u128>,
    expected_drops: usize,
) -> io::Result<()> {
    // The existing encoder validates exact v5 bytes while the stage remains
    // guarded by `.conversion-incomplete`.
    let mut actual_chunks = 0usize;
    for entry in fs::read_dir(stage)? {
        let entry = entry?;
        if entry.file_name().to_str().and_then(parse_chunk).is_some() {
            actual_chunks += 1;
        }
    }
    if actual_chunks != chunks.values().filter(|value| !value.is_empty()).count() {
        return Err(invalid("converted chunk count mismatch"));
    }
    for key in chunks.keys() {
        let path = stage.join(format!("{}_{}_{}.bged", key.x, key.y, key.z));
        if path.exists() {
            storage.decode_snapshot(Some(&read_bounded(&path, 26 + 4096 * 6 + 4)?))?;
        }
    }
    let store = InventoryStore::with_catalog(stage, Arc::clone(catalog))?;
    let mut actual_inventories = 0usize;
    for entry in fs::read_dir(stage.join("players"))? {
        let entry = entry?;
        let name = entry.file_name();
        let profile = name
            .to_str()
            .and_then(parse_profile)
            .ok_or_else(|| invalid("unexpected converted player entry"))?;
        store.load(profile)?;
        actual_inventories += 1;
    }
    if actual_inventories != *inventories {
        return Err(invalid("converted inventory count mismatch"));
    }
    let drops = Drops::open_with_catalog(stage, Arc::clone(catalog))?;
    if drops.rotation_compaction().drops.len() != expected_drops {
        return Err(invalid("converted drops count mismatch"));
    }
    let journal = Journal::open(stage.join("server.wal"))?;
    if !journal.drop_owner_set_closed() {
        return Err(invalid("converted drop owner set is not closed"));
    }
    let latest = journal.latest_values();
    drops.validate_recovered_journal(&latest, true)?;
    for &profile in receipt_profiles {
        let key = StateKey::new("bloxgloom:action_ledger", profile.to_le_bytes().to_vec());
        let checkpoint = fs::read(
            stage
                .join("receipts")
                .join(format!("{profile:032x}.ledger")),
        )?;
        if latest.get(&key) != Some(&checkpoint) {
            return Err(invalid("converted receipt frontier mismatch"));
        }
    }
    if latest
        .keys()
        .any(|key| key.domain == "bloxgloom:action_receipt")
    {
        return Err(invalid("legacy action key leaked into converted WAL"));
    }
    Ok(())
}

fn decode_chunk_key(bytes: &[u8]) -> io::Result<ChunkKey> {
    if bytes.len() != 12 {
        return Err(invalid("invalid legacy chunk WAL key"));
    }
    Ok(ChunkKey {
        x: i32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        y: i32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        z: i32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    })
}

fn decode_profile_key(bytes: &[u8]) -> io::Result<u128> {
    if bytes.len() != 16 {
        return Err(invalid("invalid legacy profile WAL key"));
    }
    let profile = u128::from_le_bytes(bytes.try_into().unwrap());
    if profile == 0 {
        return Err(invalid("zero legacy profile ID"));
    }
    Ok(profile)
}

fn decode_drop_id(bytes: &[u8]) -> io::Result<u64> {
    if bytes.len() != 8 {
        return Err(invalid("invalid legacy drop WAL key"));
    }
    let id = u64::from_le_bytes(bytes.try_into().unwrap());
    if id == 0 {
        return Err(invalid("zero legacy drop ID"));
    }
    Ok(id)
}

fn read_optional_bounded(path: &Path, limit: usize) -> io::Result<Option<Vec<u8>>> {
    match File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
            if bytes.len() > limit {
                return Err(invalid("legacy file exceeds codec limit"));
            }
            Ok(Some(bytes))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_bounded(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    read_optional_bounded(path, limit)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("required save file is missing: {}", path.display()),
        )
    })
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    File::open(
        path.parent()
            .ok_or_else(|| invalid("invalid output path"))?,
    )?
    .sync_all()
}

fn copy_new(source: &Path, destination: &Path) -> io::Result<()> {
    let bytes = fs::read(source)?;
    write_new(destination, &bytes)
}

/// Publish without replacing even an empty directory created by another
/// process after the final preflight check.
#[cfg(target_os = "macos")]
fn rename_new(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::{CString, c_char, c_int, c_uint};
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn renamex_np(from: *const c_char, to: *const c_char, flags: c_uint) -> c_int;
    }
    const RENAME_EXCL: c_uint = 0x0000_0004;
    let from =
        CString::new(source.as_os_str().as_bytes()).map_err(|_| invalid("invalid source path"))?;
    let to = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| invalid("invalid destination path"))?;
    // SAFETY: both NUL-terminated path buffers remain alive through the call.
    if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), RENAME_EXCL) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn rename_new(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::{CString, c_char, c_int, c_uint};
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn renameat2(
            old_fd: c_int,
            old: *const c_char,
            new_fd: c_int,
            new: *const c_char,
            flags: c_uint,
        ) -> c_int;
    }
    const AT_FDCWD: c_int = -100;
    const RENAME_NOREPLACE: c_uint = 1;
    let from =
        CString::new(source.as_os_str().as_bytes()).map_err(|_| invalid("invalid source path"))?;
    let to = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| invalid("invalid destination path"))?;
    // SAFETY: both NUL-terminated path buffers remain alive through the call.
    if unsafe {
        renameat2(
            AT_FDCWD,
            from.as_ptr(),
            AT_FDCWD,
            to.as_ptr(),
            RENAME_NOREPLACE,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn rename_new(source: &Path, destination: &Path) -> io::Result<()> {
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }
    fs::rename(source, destination)
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

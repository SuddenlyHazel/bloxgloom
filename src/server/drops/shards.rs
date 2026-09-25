//! Per-chunk drop checkpoint shards and the allocator checkpoint.
//!
//! The legacy `drops.bin` snapshot is O(all drops): one moving drop forces
//! the coordinator to serialize and sort every drop. Shards split that work
//! by chunk owner so a checkpoint only touches dirty chunks. Each shard file
//! carries its own framing and checksum and fails closed on corruption;
//! `drops.bin` remains readable as a legacy source when no shard directory
//! exists.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::inventory::{ComponentPayload, MAX_COMPONENT_BYTES, STACK_LIMIT, Stack};
use crate::items::ItemId;
use crate::world::ChunkKey;

use super::persistence::RECORD;
use super::{DropEntityPayload, Drops, Entry, chunk_of, invalid};

pub(super) const SHARD_MAGIC: &[u8; 4] = b"BGDC";
pub(super) const SHARD_FORMAT: u16 = 1;
pub(super) const SHARD_HEADER: usize = 4 + 2 + 12 + 4;
pub(super) const MAX_SHARD_BYTES: usize = 64 * 1024 * 1024;

pub(super) const ALLOC_MAGIC: &[u8; 4] = b"BGDA";
pub(super) const ALLOC_FORMAT: u16 = 1;
const ALLOC_BODY: usize = 8 + 8;
const ALLOC_LEN: usize = 4 + 2 + ALLOC_BODY + 4;
/// Encoded allocator checkpoint length, reserved at WAL staging time.
pub(in crate::server) const SHARD_ALLOCATOR_LEN: usize = ALLOC_LEN;

pub(in crate::server) fn shard_dir_for_drops_file(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("drops.d"),
        _ => PathBuf::from("drops.d"),
    }
}

pub(in crate::server) fn shard_path(dir: &Path, chunk: ChunkKey) -> PathBuf {
    dir.join(format!(
        "chunk_{}_{}_{}.bin",
        chunk.x, chunk.y, chunk.z
    ))
}

pub(in crate::server) fn allocator_path(dir: &Path) -> PathBuf {
    dir.join("allocator.bin")
}

/// Serializes one chunk owner's drops. Callers pass members in ID order so
/// shard bytes are deterministic for a given chunk state.
pub(super) fn encode_shard(chunk: ChunkKey, entries: &[&Entry]) -> io::Result<Vec<u8>> {
    if entries.len() > 1_000_000 {
        return Err(invalid("too many drops in one chunk"));
    }
    let mut bytes = Vec::with_capacity(
        SHARD_HEADER
            + entries
                .iter()
                .map(|entry| {
                    RECORD
                        + entry
                            .drop_payload()
                            .stack
                            .components
                            .as_ref()
                            .map_or(0, |component| component.bytes.len())
                })
                .sum::<usize>()
            + 4,
    );
    bytes.extend(SHARD_MAGIC);
    bytes.extend(SHARD_FORMAT.to_le_bytes());
    bytes.extend(chunk.x.to_le_bytes());
    bytes.extend(chunk.y.to_le_bytes());
    bytes.extend(chunk.z.to_le_bytes());
    bytes.extend((entries.len() as u32).to_le_bytes());
    for entry in entries {
        encode_record(&mut bytes, entry);
    }
    if bytes.len() > MAX_SHARD_BYTES {
        return Err(invalid("drop shard snapshot too large"));
    }
    bytes.extend(checksum(&bytes).to_le_bytes());
    Ok(bytes)
}

fn encode_record(bytes: &mut Vec<u8>, entry: &Entry) {
    let payload = entry.drop_payload();
    bytes.extend(entry.id.to_le_bytes());
    bytes.extend(payload.stack.item.get().to_le_bytes());
    bytes.extend(payload.stack.count.to_le_bytes());
    for n in entry.position {
        bytes.extend(n.to_le_bytes());
    }
    bytes.extend(payload.created_unix_ms.to_le_bytes());
    bytes.extend((payload.pickup_delay.as_millis().min(u16::MAX as u128) as u16).to_le_bytes());
    if let Some(component) = &payload.stack.components {
        bytes.extend(component.version.to_le_bytes());
        bytes.extend((component.bytes.len() as u16).to_le_bytes());
        bytes.extend(&component.bytes);
    } else {
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
    }
}

pub(super) struct DecodedShard {
    pub chunk: ChunkKey,
    pub drops: Vec<(u64, [f32; 3], DropEntityPayload)>,
}

pub(super) fn decode_shard(
    bytes: &[u8],
    catalog: &crate::content::Catalog,
) -> io::Result<DecodedShard> {
    if bytes.len() < SHARD_HEADER + 4
        || &bytes[..4] != SHARD_MAGIC
        || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != SHARD_FORMAT
    {
        return Err(invalid("invalid drop shard"));
    }
    let chunk = ChunkKey {
        x: i32::from_le_bytes(bytes[6..10].try_into().unwrap()),
        y: i32::from_le_bytes(bytes[10..14].try_into().unwrap()),
        z: i32::from_le_bytes(bytes[14..18].try_into().unwrap()),
    };
    let count = u32::from_le_bytes(bytes[18..22].try_into().unwrap()) as usize;
    if count > 1_000_000 || bytes.len() > MAX_SHARD_BYTES {
        return Err(invalid("invalid drop shard length"));
    }
    let checksum_at = bytes.len() - 4;
    if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
        != checksum(&bytes[..checksum_at])
    {
        return Err(invalid("drop shard checksum mismatch"));
    }
    let mut drops = Vec::with_capacity(count.min(1024));
    let mut offset = SHARD_HEADER;
    for _ in 0..count {
        let record_end = offset
            .checked_add(RECORD)
            .ok_or_else(|| invalid("invalid drop shard length"))?;
        let record = bytes
            .get(offset..record_end)
            .ok_or_else(|| invalid("truncated drop shard record"))?;
        let component_len = u16::from_le_bytes(record[38..40].try_into().unwrap()) as usize;
        if component_len > MAX_COMPONENT_BYTES {
            return Err(invalid("invalid drop shard component length"));
        }
        let component_end = record_end
            .checked_add(component_len)
            .ok_or_else(|| invalid("invalid drop shard length"))?;
        let component_bytes = bytes
            .get(record_end..component_end)
            .ok_or_else(|| invalid("truncated drop shard components"))?;
        let (id, position, payload) =
            decode_record(record, component_bytes, catalog).map_err(|_| invalid("invalid drop shard item"))?;
        if chunk_of(position) != chunk {
            return Err(invalid("drop shard member belongs to another chunk"));
        }
        drops.push((id, position, payload));
        offset = component_end;
    }
    if offset != checksum_at {
        return Err(invalid("invalid drop shard record count"));
    }
    Ok(DecodedShard { chunk, drops })
}

fn decode_record(
    record: &[u8],
    component_bytes: &[u8],
    catalog: &crate::content::Catalog,
) -> Result<(u64, [f32; 3], DropEntityPayload), ()> {
    let component_version = u16::from_le_bytes(record[36..38].try_into().map_err(|_| ())?);
    let components = if component_bytes.is_empty() {
        if component_version != 0 {
            return Err(());
        }
        None
    } else {
        Some(std::sync::Arc::new(
            ComponentPayload::new(component_version, component_bytes.to_vec()).ok_or(())?,
        ))
    };
    let id = u64::from_le_bytes(record[0..8].try_into().map_err(|_| ())?);
    let item = ItemId::new(u32::from_le_bytes(record[8..12].try_into().map_err(|_| ())?));
    let count = u16::from_le_bytes(record[12..14].try_into().map_err(|_| ())?);
    let position = [
        f32::from_le_bytes(record[14..18].try_into().map_err(|_| ())?),
        f32::from_le_bytes(record[18..22].try_into().map_err(|_| ())?),
        f32::from_le_bytes(record[22..26].try_into().map_err(|_| ())?),
    ];
    let born = u64::from_le_bytes(record[26..34].try_into().map_err(|_| ())?);
    let delay = u16::from_le_bytes(record[34..36].try_into().map_err(|_| ())?);
    if id == 0
        || catalog.item(item).is_none()
        || !(1..=STACK_LIMIT).contains(&count)
        || position.iter().any(|n| !n.is_finite())
    {
        return Err(());
    }
    Ok((
        id,
        position,
        DropEntityPayload::new(
            Stack {
                item,
                count,
                components,
            },
            born,
            Duration::from_millis(u64::from(delay)),
        ),
    ))
}

pub(super) fn encode_allocator(next_id: u64, revision: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(ALLOC_LEN);
    bytes.extend(ALLOC_MAGIC);
    bytes.extend(ALLOC_FORMAT.to_le_bytes());
    bytes.extend(next_id.to_le_bytes());
    bytes.extend(revision.to_le_bytes());
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

pub(super) fn decode_allocator(bytes: &[u8]) -> io::Result<(u64, u64)> {
    if bytes.len() != ALLOC_LEN
        || &bytes[..4] != ALLOC_MAGIC
        || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != ALLOC_FORMAT
    {
        return Err(invalid("invalid drop allocator checkpoint"));
    }
    let checksum_at = bytes.len() - 4;
    if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
        != checksum(&bytes[..checksum_at])
    {
        return Err(invalid("drop allocator checksum mismatch"));
    }
    let next_id = u64::from_le_bytes(bytes[6..14].try_into().unwrap());
    let revision = u64::from_le_bytes(bytes[14..22].try_into().unwrap());
    if next_id == 0 {
        return Err(invalid("invalid drop allocator checkpoint"));
    }
    Ok((next_id, revision))
}

/// Validates BGDC framing before replacing the shard file. An empty shard
/// deletes its file instead of writing one: a missing shard loads as an
/// empty owner set, and the WAL journal overlay stays authoritative for
/// ownership either way.
pub(in crate::server) fn write_shard_snapshot(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() < SHARD_HEADER + 4
        || &bytes[..4] != SHARD_MAGIC
        || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != SHARD_FORMAT
    {
        return Err(invalid("invalid drop shard checkpoint"));
    }
    let count = u32::from_le_bytes(bytes[18..22].try_into().unwrap()) as usize;
    if count > 1_000_000 || bytes.len() > MAX_SHARD_BYTES {
        return Err(invalid("invalid drop shard checkpoint length"));
    }
    let checksum_at = bytes.len() - 4;
    let mut offset = SHARD_HEADER;
    for _ in 0..count {
        let record_end = offset
            .checked_add(RECORD)
            .ok_or_else(|| invalid("invalid drop shard checkpoint length"))?;
        let record = bytes
            .get(offset..record_end)
            .ok_or_else(|| invalid("truncated drop shard checkpoint"))?;
        let len = u16::from_le_bytes(record[38..40].try_into().unwrap()) as usize;
        if len > MAX_COMPONENT_BYTES {
            return Err(invalid(
                "invalid drop shard checkpoint component length",
            ));
        }
        offset = record_end
            .checked_add(len)
            .ok_or_else(|| invalid("invalid drop shard checkpoint length"))?;
        if offset > checksum_at {
            return Err(invalid("truncated drop shard checkpoint components"));
        }
    }
    if offset != checksum_at {
        return Err(invalid("invalid drop shard checkpoint record count"));
    }
    if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
        != checksum(&bytes[..checksum_at])
    {
        return Err(invalid("drop shard checkpoint checksum mismatch"));
    }
    if count == 0 {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
        if let Some(parent) = path.parent() {
            File::open(parent)?.sync_all()?;
        }
        return Ok(());
    }
    atomic_replace(path, bytes)
}

pub(in crate::server) fn write_allocator_snapshot(path: &Path, bytes: &[u8]) -> io::Result<()> {
    decode_allocator(bytes)?;
    atomic_replace(path, bytes)
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| invalid("invalid drop shard path"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".drops-shard.{}.{}.tmp",
        std::process::id(),
        super::persistence::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub(super) struct LoadedShards {
    pub drops: Vec<(u64, [f32; 3], DropEntityPayload)>,
    pub next_id: u64,
    pub revision: u64,
}

/// Loads the sharded checkpoint base. Returns `None` when no shard marker
/// exists so the caller falls back to the legacy `drops.bin` source.
pub(super) fn load_sharded(
    dir: &Path,
    catalog: &crate::content::Catalog,
) -> Option<io::Result<LoadedShards>> {
    let listing = match fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => return Some(Err(error)),
    };
    let mut chunk_files = Vec::new();
    let mut has_allocator = false;
    for entry in listing {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return Some(Err(error)),
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if name == "allocator.bin" {
            has_allocator = true;
        } else if name.starts_with("chunk_") && name.ends_with(".bin") {
            chunk_files.push(entry.path());
        }
    }
    if !has_allocator && chunk_files.is_empty() {
        return None;
    }
    Some(load_sharded_inner(dir, chunk_files, has_allocator, catalog))
}

fn load_sharded_inner(
    dir: &Path,
    mut chunk_files: Vec<PathBuf>,
    has_allocator: bool,
    catalog: &crate::content::Catalog,
) -> io::Result<LoadedShards> {
    // A missing allocator file is reconstructed from shard IDs: every save
    // writes the allocator first, so its absence means an interrupted save
    // whose shard writes may also lag. The journal allocator overlay takes
    // the max on recovery and rejects a stale value, so this cannot reuse
    // an ID. A corrupt allocator file still fails closed.
    let (mut next_id, revision) = if has_allocator {
        let allocator_bytes = fs::read(allocator_path(dir))?;
        decode_allocator(&allocator_bytes)?
    } else {
        (1, 0)
    };
    chunk_files.sort();
    let mut drops = Vec::new();
    let mut seen = BTreeSet::new();
    for path in &chunk_files {
        let expected = parse_chunk_file_name(path).ok_or_else(|| invalid("invalid drop shard name"))?;
        let decoded = decode_shard(&fs::read(path)?, catalog)?;
        if decoded.chunk != expected {
            return Err(invalid("drop shard file name disagrees with its chunk"));
        }
        for (id, position, payload) in decoded.drops {
            if !seen.insert(id) {
                return Err(invalid("duplicate drop ID across shards"));
            }
            next_id = next_id.max(id.saturating_add(1));
            drops.push((id, position, payload));
        }
    }
    next_id = next_id.max(1);
    Ok(LoadedShards {
        drops,
        next_id,
        revision,
    })
}

fn parse_chunk_file_name(path: &Path) -> Option<ChunkKey> {
    let name = path.file_name()?.to_string_lossy();
    let coords = name.strip_prefix("chunk_")?.strip_suffix(".bin")?;
    let mut parts = coords.split('_');
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    let z = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(ChunkKey { x, y, z })
}

/// Deletes shard files for chunks with no live drops. Chunks that lose
/// their last drop normally delete their file through the checkpoint
/// write; this only catches leftovers from an interrupted earlier save.
/// Unknown and dotfiles (worker temporaries) are left alone.
pub(super) fn sweep_stale_shards(
    dir: &Path,
    live: &BTreeMap<ChunkKey, BTreeSet<u64>>,
) -> io::Result<()> {
    let listing = match fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut removed = false;
    for entry in listing {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "allocator.bin" {
            continue;
        }
        let Some(chunk) = parse_chunk_file_name(&entry.path()) else {
            continue;
        };
        if !live.contains_key(&chunk) {
            fs::remove_file(entry.path())?;
            removed = true;
        }
    }
    if removed {
        File::open(dir)?.sync_all()?;
    }
    Ok(())
}

pub(super) fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}

impl Drops {
    /// Drains the dirty-chunk set into per-chunk snapshot bytes in chunk
    /// order. One moving drop serializes only its chunk owners, never the
    /// whole drop set.
    pub(in crate::server) fn take_dirty_shard_snapshots(&mut self) -> Vec<(ChunkKey, Vec<u8>)> {
        let chunks: Vec<ChunkKey> = std::mem::take(&mut self.chunk_dirty)
            .into_iter()
            .collect();
        let mut snapshots = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let members = self.chunk_members.get(&chunk);
            let mut entries: Vec<&Entry> = members
                .map(|set| {
                    set.iter()
                        .filter_map(|member| self.entries.get(member))
                        .collect()
                })
                .unwrap_or_default();
            entries.sort_by_key(|entry| entry.id);
            // Encoding cannot fail on live entries: they were validated at
            // insertion and only change through validated plans.
            let bytes = encode_shard(chunk, &entries).expect("live drop shard encodes");
            snapshots.push((chunk, bytes));
        }
        snapshots
    }

    /// Drains the allocator-dirty flag into allocator checkpoint bytes.
    pub(in crate::server) fn take_allocator_snapshot(&mut self) -> Option<Vec<u8>> {
        if !self.allocator_dirty {
            return None;
        }
        self.allocator_dirty = false;
        Some(encode_allocator(self.next_id, self.revision))
    }

    /// Checkpoint directory for this store's shards, if it has a save path.
    pub(in crate::server) fn shard_dir(&self) -> Option<PathBuf> {
        self.path
            .as_ref()
            .map(|path| shard_dir_for_drops_file(path))
    }

    /// Shard file for one chunk owner, if this store has a save path.
    pub(in crate::server) fn shard_snapshot_path(&self, chunk: ChunkKey) -> Option<PathBuf> {
        self.shard_dir().map(|dir| shard_path(&dir, chunk))
    }

    /// Allocator checkpoint file, if this store has a save path.
    pub(in crate::server) fn allocator_snapshot_path(&self) -> Option<PathBuf> {
        self.shard_dir().map(|dir| allocator_path(&dir))
    }

    /// Re-queues shard snapshots the checkpoint backlog refused, so the
    /// next attempt retries them instead of dropping work.
    pub(in crate::server) fn restore_shard_dirtiness(
        &mut self,
        chunks: Vec<ChunkKey>,
        allocator: bool,
    ) {
        self.chunk_dirty.extend(chunks);
        self.allocator_dirty |= allocator;
    }

    /// True while shard snapshots still wait for checkpoint submission.
    /// The receipt path clears the coordinator dirty flags only once this
    /// and every submitted drops key have drained.
    pub(in crate::server) fn has_uncheckpointed_shards(&self) -> bool {
        !self.chunk_dirty.is_empty() || self.allocator_dirty
    }
}

//! Versioned BGDP snapshot framing, validation, and atomic checkpoint writes.
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::inventory::{ComponentPayload, MAX_COMPONENT_BYTES, STACK_LIMIT, Stack};
use crate::items::ItemId;
use crate::protocol::DroppedItem;

use super::{
    DropEntityPayload, DropPlan, Drops, Entry, LIFETIME, expiry, invalid, spatial, unix_ms,
};

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;

pub(super) const MAGIC: &[u8; 4] = b"BGDP";
pub(super) const FORMAT: u16 = 3;
pub(super) const HEADER: usize = 4 + 2 + 8 + 8 + 4;
pub(super) const RECORD: usize = 8 + 4 + 2 + 12 + 8 + 2 + 2 + 2;
const MAX_SNAPSHOT_BYTES: usize = 256 * 1024 * 1024;
pub(super) static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl Drops {
    pub(in crate::server) fn open(root: &Path) -> io::Result<Self> {
        Self::open_with_catalog(root, Arc::new(crate::content::catalog().clone()))
    }

    pub(in crate::server) fn open_with_catalog(
        root: &Path,
        catalog: Arc<crate::content::Catalog>,
    ) -> io::Result<Self> {
        let path = root.join("drops.bin");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut drops = Self::new_with_catalog(catalog);
                drops.path = Some(path);
                return Ok(drops);
            }
            Err(error) => return Err(error),
        };
        if bytes.len() < HEADER + 4
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != FORMAT
        {
            return Err(invalid("invalid drops file"));
        }
        let count = u32::from_le_bytes(bytes[22..26].try_into().unwrap()) as usize;
        if count > 1_000_000
            || bytes.len() > MAX_SNAPSHOT_BYTES
            || bytes.len() < HEADER + count * RECORD + 4
        {
            return Err(invalid("invalid drops length"));
        }
        let checksum_at = bytes.len() - 4;
        if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
            != checksum(&bytes[..checksum_at])
        {
            return Err(invalid("drops checksum mismatch"));
        }
        let mut drops = Self {
            entries: HashMap::with_capacity(count),
            active: BTreeSet::new(),
            spatial: spatial::DropSpatialIndex::new(),
            expiry: expiry::ExpiryIndex::default(),
            next_id: u64::from_le_bytes(bytes[14..22].try_into().unwrap()),
            revision: u64::from_le_bytes(bytes[6..14].try_into().unwrap()),
            path: Some(path),
            catalog,
            last_gc: Instant::now(),
        };
        let now_ms = unix_ms();
        let mut offset = HEADER;
        for _ in 0..count {
            let record_end = offset
                .checked_add(RECORD)
                .ok_or_else(|| invalid("invalid drops length"))?;
            let record = bytes
                .get(offset..record_end)
                .ok_or_else(|| invalid("truncated drop record"))?;
            let component_len = u16::from_le_bytes(record[38..40].try_into().unwrap()) as usize;
            if component_len > MAX_COMPONENT_BYTES {
                return Err(invalid("invalid drop component length"));
            }
            let component_end = record_end
                .checked_add(component_len)
                .ok_or_else(|| invalid("invalid drops length"))?;
            let component_bytes = bytes
                .get(record_end..component_end)
                .ok_or_else(|| invalid("truncated drop components"))?;
            let component_version = u16::from_le_bytes(record[36..38].try_into().unwrap());
            let components = if component_len == 0 {
                if component_version != 0 {
                    return Err(invalid("invalid empty drop components"));
                }
                None
            } else {
                Some(Arc::new(
                    ComponentPayload::new(component_version, component_bytes.to_vec())
                        .ok_or_else(|| invalid("invalid drop components"))?,
                ))
            };
            let item = DroppedItem {
                id: u64::from_le_bytes(record[0..8].try_into().unwrap()),
                item: ItemId::new(u32::from_le_bytes(record[8..12].try_into().unwrap())),
                count: u16::from_le_bytes(record[12..14].try_into().unwrap()),
                position: [
                    f32::from_le_bytes(record[14..18].try_into().unwrap()),
                    f32::from_le_bytes(record[18..22].try_into().unwrap()),
                    f32::from_le_bytes(record[22..26].try_into().unwrap()),
                ],
                age_ms: 0,
            };
            let born = u64::from_le_bytes(record[26..34].try_into().unwrap());
            let delay = u16::from_le_bytes(record[34..36].try_into().unwrap());
            if item.id == 0
                || drops.catalog.item(item.item).is_none()
                || !(1..=STACK_LIMIT).contains(&item.count)
                || item.position.iter().any(|n| !n.is_finite())
            {
                return Err(invalid("invalid dropped item"));
            }
            let age = Duration::from_millis(now_ms.saturating_sub(born));
            let age_since = Instant::now();
            let payload = DropEntityPayload::new(
                Stack {
                    item: item.item,
                    count: item.count,
                    components,
                },
                born,
                Duration::from_millis(u64::from(delay)),
            );
            if drops
                .entries
                .insert(
                    item.id,
                    Entry::new(item.id, item.position, payload, 0.0, age, age_since),
                )
                .is_some()
            {
                return Err(invalid("duplicate drop ID"));
            }
            drops.next_id = drops.next_id.max(item.id.saturating_add(1));
            drops.spatial.insert(item.id, item.position);
            drops.expiry.insert(item.id, age, age_since);
            if age < LIFETIME {
                drops.active.insert(item.id);
            }
            offset = component_end;
        }
        if offset != checksum_at {
            return Err(invalid("invalid drops record count"));
        }
        drops.next_id = drops.next_id.max(1);
        Ok(drops)
    }

    pub(in crate::server) fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        Self::write_snapshot(path, &self.snapshot_bytes()?)
    }

    /// Captures the exact BGDP checkpoint, including motion position and components.
    /// Runtime callers send these bytes through the shared checkpoint worker.
    pub(in crate::server) fn snapshot_bytes(&self) -> io::Result<Vec<u8>> {
        if self.entries.len() > 1_000_000 {
            return Err(invalid("too many drops"));
        }
        let projected = HEADER
            + self
                .entries
                .values()
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
            + 4;
        if projected > MAX_SNAPSHOT_BYTES {
            return Err(invalid("drops snapshot too large"));
        }
        let mut bytes = Vec::with_capacity(projected);
        bytes.extend(MAGIC);
        bytes.extend(FORMAT.to_le_bytes());
        bytes.extend(self.revision.to_le_bytes());
        bytes.extend(self.next_id.to_le_bytes());
        bytes.extend((self.entries.len() as u32).to_le_bytes());
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by_key(|entry| entry.id);
        for entry in entries {
            let payload = entry.drop_payload();
            bytes.extend(entry.id.to_le_bytes());
            bytes.extend(payload.stack.item.get().to_le_bytes());
            bytes.extend(payload.stack.count.to_le_bytes());
            for n in entry.position {
                bytes.extend(n.to_le_bytes());
            }
            bytes.extend(payload.created_unix_ms.to_le_bytes());
            bytes.extend(
                (payload.pickup_delay.as_millis().min(u16::MAX as u128) as u16).to_le_bytes(),
            );
            if let Some(component) = &payload.stack.components {
                bytes.extend(component.version.to_le_bytes());
                bytes.extend((component.bytes.len() as u16).to_le_bytes());
                bytes.extend(&component.bytes);
            } else {
                bytes.extend(0u16.to_le_bytes());
                bytes.extend(0u16.to_le_bytes());
            }
        }
        bytes.extend(checksum(&bytes).to_le_bytes());
        Ok(bytes)
    }

    /// All serialized drop mutations advance `revision`. A checkpoint receipt
    /// can therefore compare this small header token instead of sorting and
    /// reserializing every drop a second time on the simulation thread.
    pub(in crate::server) fn matches_checkpoint_generation(&self, bytes: &[u8]) -> bool {
        if bytes.len() < HEADER + 4
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != FORMAT
        {
            return false;
        }
        let revision = u64::from_le_bytes(bytes[6..14].try_into().unwrap());
        let next_id = u64::from_le_bytes(bytes[14..22].try_into().unwrap());
        let count = u32::from_le_bytes(bytes[22..26].try_into().unwrap()) as usize;
        bytes.len() >= HEADER + count * RECORD + 4
            && bytes.len() <= MAX_SNAPSHOT_BYTES
            && revision == self.revision
            && next_id == self.next_id
            && count == self.entries.len()
    }

    pub(in crate::server) fn projected_snapshot_size(&self, plan: &DropPlan) -> io::Result<usize> {
        self.validate_plan(plan)?;
        let mut count = self.entries.len();
        for mutation in &plan.changes {
            let exists = self.entries.contains_key(&mutation.id);
            match (exists, mutation.after.is_empty()) {
                (false, false) => count += 1,
                (true, true) => count -= 1,
                _ => {}
            }
        }
        if count > 1_000_000 {
            return Err(invalid("too many drops"));
        }
        let mut size = HEADER + 4;
        for entry in self.entries.values() {
            size += RECORD
                + entry
                    .drop_payload()
                    .stack
                    .components
                    .as_ref()
                    .map_or(0, |component| component.bytes.len());
        }
        for mutation in &plan.changes {
            let old = self.entries.get(&mutation.id).map_or(0, |entry| {
                RECORD
                    + entry
                        .drop_payload()
                        .stack
                        .components
                        .as_ref()
                        .map_or(0, |component| component.bytes.len())
            });
            let new = if mutation.after.is_empty() {
                0
            } else {
                RECORD + mutation.after.len() - 21
            };
            size = size - old + new;
        }
        if size > MAX_SNAPSHOT_BYTES {
            return Err(invalid("drops snapshot too large"));
        }
        Ok(size)
    }

    pub(in crate::server) fn checkpoint_path(&self) -> Option<PathBuf> {
        self.path.clone()
    }

    pub(in crate::server) fn write_snapshot(path: &Path, bytes: &[u8]) -> io::Result<()> {
        // Every submitted snapshot is generated by `snapshot_bytes`; also
        // validate the framing here so a malformed caller cannot replace a
        // good checkpoint.
        if bytes.len() < HEADER + 4
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != FORMAT
        {
            return Err(invalid("invalid drops checkpoint"));
        }
        let count = u32::from_le_bytes(bytes[22..26].try_into().unwrap()) as usize;
        if count > 1_000_000
            || bytes.len() > MAX_SNAPSHOT_BYTES
            || bytes.len() < HEADER + count * RECORD + 4
        {
            return Err(invalid("invalid drops checkpoint length"));
        }
        let checksum_at = bytes.len() - 4;
        let mut offset = HEADER;
        for _ in 0..count {
            let record_end = offset
                .checked_add(RECORD)
                .ok_or_else(|| invalid("invalid drops checkpoint length"))?;
            let record = bytes
                .get(offset..record_end)
                .ok_or_else(|| invalid("truncated drops checkpoint"))?;
            let len = u16::from_le_bytes(record[38..40].try_into().unwrap()) as usize;
            if len > MAX_COMPONENT_BYTES {
                return Err(invalid("invalid drops checkpoint component length"));
            }
            offset = record_end
                .checked_add(len)
                .ok_or_else(|| invalid("invalid drops checkpoint length"))?;
            if offset > checksum_at {
                return Err(invalid("truncated drops checkpoint components"));
            }
        }
        if offset != checksum_at {
            return Err(invalid("invalid drops checkpoint record count"));
        }
        if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
            != checksum(&bytes[..checksum_at])
        {
            return Err(invalid("drops checkpoint checksum mismatch"));
        }
        let parent = path.parent().ok_or_else(|| invalid("invalid drops path"))?;
        let temporary = parent.join(format!(
            ".drops.{}.{}.tmp",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
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
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}

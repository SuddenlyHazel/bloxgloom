//! Legacy BGDP snapshot framing, validation, and atomic checkpoint writes.
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::inventory::STACK_LIMIT;
use crate::items::valid_item;
use crate::protocol::DroppedItem;

use super::{DropPlan, Drops, Entry, LIFETIME, expiry, invalid, spatial, unix_ms};

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;

pub(super) const MAGIC: &[u8; 4] = b"BGDP";
pub(super) const FORMAT: u16 = 1;
pub(super) const HEADER: usize = 4 + 2 + 8 + 8 + 4;
pub(super) const RECORD: usize = 8 + 1 + 2 + 12 + 8 + 2;
pub(super) static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl Drops {
    pub(in crate::server) fn open(root: &Path) -> io::Result<Self> {
        let path = root.join("drops.bin");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut drops = Self::new();
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
        if count > 1_000_000 || bytes.len() != HEADER + count * RECORD + 4 {
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
            last_gc: Instant::now(),
        };
        let now_ms = unix_ms();
        for record in bytes[HEADER..checksum_at].chunks_exact(RECORD) {
            let item = DroppedItem {
                id: u64::from_le_bytes(record[0..8].try_into().unwrap()),
                item: record[8],
                count: u16::from_le_bytes(record[9..11].try_into().unwrap()),
                position: [
                    f32::from_le_bytes(record[11..15].try_into().unwrap()),
                    f32::from_le_bytes(record[15..19].try_into().unwrap()),
                    f32::from_le_bytes(record[19..23].try_into().unwrap()),
                ],
                age_ms: 0,
            };
            let born = u64::from_le_bytes(record[23..31].try_into().unwrap());
            let delay = u16::from_le_bytes(record[31..33].try_into().unwrap());
            if item.id == 0
                || !valid_item(item.item)
                || !(1..=STACK_LIMIT).contains(&item.count)
                || item.position.iter().any(|n| !n.is_finite())
            {
                return Err(invalid("invalid dropped item"));
            }
            let age = Duration::from_millis(now_ms.saturating_sub(born));
            let age_since = Instant::now();
            if drops
                .entries
                .insert(
                    item.id,
                    Entry {
                        item,
                        vertical_speed: 0.0,
                        age_at_load: age,
                        age_since,
                        created_unix_ms: born,
                        pickup_delay: Duration::from_millis(u64::from(delay)),
                    },
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

    /// Captures the exact legacy BGDP checkpoint, including motion position.
    /// Runtime callers send these bytes through the shared checkpoint worker.
    pub(in crate::server) fn snapshot_bytes(&self) -> io::Result<Vec<u8>> {
        if self.entries.len() > 1_000_000 {
            return Err(invalid("too many drops"));
        }
        let mut bytes = Vec::with_capacity(HEADER + self.entries.len() * RECORD + 4);
        bytes.extend(MAGIC);
        bytes.extend(FORMAT.to_le_bytes());
        bytes.extend(self.revision.to_le_bytes());
        bytes.extend(self.next_id.to_le_bytes());
        bytes.extend((self.entries.len() as u32).to_le_bytes());
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by_key(|entry| entry.item.id);
        for entry in entries {
            let item = entry.item;
            bytes.extend(item.id.to_le_bytes());
            bytes.push(item.item);
            bytes.extend(item.count.to_le_bytes());
            for n in item.position {
                bytes.extend(n.to_le_bytes());
            }
            bytes.extend(entry.created_unix_ms.to_le_bytes());
            bytes.extend(
                (entry.pickup_delay.as_millis().min(u16::MAX as u128) as u16).to_le_bytes(),
            );
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
        count
            .checked_mul(RECORD)
            .and_then(|records| records.checked_add(HEADER + 4))
            == Some(bytes.len())
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
        Ok(HEADER + count * RECORD + 4)
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
        if count > 1_000_000 || bytes.len() != HEADER + count * RECORD + 4 {
            return Err(invalid("invalid drops checkpoint length"));
        }
        let checksum_at = bytes.len() - 4;
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

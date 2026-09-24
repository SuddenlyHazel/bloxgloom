//! Bounded world drop snapshots and deterministic pickup candidates.
use crate::inventory::STACK_LIMIT;
use crate::protocol::DroppedItem;
use crate::world::MAX_BLOCK;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAGIC: &[u8; 4] = b"BGDP";
const FORMAT: u16 = 1;
const HEADER: usize = 4 + 2 + 8 + 8 + 4;
const RECORD: usize = 8 + 1 + 2 + 12 + 8 + 2;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const PICKUP_RANGE_SQ: f32 = 2.25 * 2.25;
const VIEW_RANGE_SQ: f32 = 64.0 * 64.0;
const LIFETIME: Duration = Duration::from_secs(600);

#[derive(Clone)]
struct Entry {
    item: DroppedItem,
    created: Instant,
    created_unix_ms: u64,
    pickup_delay: Duration,
}

#[derive(Clone)]
pub(super) struct Drops {
    entries: HashMap<u64, Entry>,
    next_id: u64,
    revision: u64,
    path: Option<PathBuf>,
    last_gc: Instant,
}

impl Drops {
    pub(super) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            next_id: 1,
            revision: 0,
            path: None,
            last_gc: Instant::now(),
        }
    }
    pub(super) fn open(root: &Path) -> io::Result<Self> {
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
            next_id: u64::from_le_bytes(bytes[14..22].try_into().unwrap()),
            revision: u64::from_le_bytes(bytes[6..14].try_into().unwrap()),
            path: Some(path),
            last_gc: Instant::now(),
        };
        let now_ms = unix_ms();
        for record in bytes[HEADER..checksum_at].chunks_exact(RECORD) {
            let item = DroppedItem {
                id: u64::from_le_bytes(record[0..8].try_into().unwrap()),
                block: record[8],
                count: u16::from_le_bytes(record[9..11].try_into().unwrap()),
                position: [
                    f32::from_le_bytes(record[11..15].try_into().unwrap()),
                    f32::from_le_bytes(record[15..19].try_into().unwrap()),
                    f32::from_le_bytes(record[19..23].try_into().unwrap()),
                ],
            };
            let born = u64::from_le_bytes(record[23..31].try_into().unwrap());
            let delay = u16::from_le_bytes(record[31..33].try_into().unwrap());
            if item.id == 0
                || !(1..=MAX_BLOCK).contains(&item.block)
                || !(1..=STACK_LIMIT).contains(&item.count)
                || item.position.iter().any(|n| !n.is_finite())
            {
                return Err(invalid("invalid dropped item"));
            }
            let age = Duration::from_millis(now_ms.saturating_sub(born));
            if age >= LIFETIME {
                continue;
            }
            let created = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
            if drops
                .entries
                .insert(
                    item.id,
                    Entry {
                        item,
                        created,
                        created_unix_ms: born,
                        pickup_delay: Duration::from_millis(u64::from(delay)),
                    },
                )
                .is_some()
            {
                return Err(invalid("duplicate drop ID"));
            }
            drops.next_id = drops.next_id.max(item.id.saturating_add(1));
        }
        drops.next_id = drops.next_id.max(1);
        Ok(drops)
    }

    pub(super) fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
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
            bytes.push(item.block);
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
            file.write_all(&bytes)?;
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
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn spawn(
        &mut self,
        position: [f32; 3],
        block: u8,
        mut count: u16,
        pickup_delay: Duration,
    ) {
        while count > 0 {
            if let Some(entry) = self.entries.values_mut().find(|entry| {
                entry.item.block == block
                    && entry.item.count < STACK_LIMIT
                    && distance_sq(entry.item.position, position) < 1.0
            }) {
                let taken = count.min(STACK_LIMIT - entry.item.count);
                entry.item.count += taken;
                entry.created = Instant::now();
                entry.created_unix_ms = unix_ms();
                count -= taken;
                self.revision = self.revision.wrapping_add(1);
                continue;
            }
            let taken = count.min(STACK_LIMIT);
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            self.entries.insert(
                id,
                Entry {
                    item: DroppedItem {
                        id,
                        block,
                        count: taken,
                        position,
                    },
                    created: Instant::now(),
                    created_unix_ms: unix_ms(),
                    pickup_delay,
                },
            );
            count -= taken;
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub(super) fn nearby(&self, position: [f32; 3]) -> Vec<DroppedItem> {
        let mut items: Vec<_> = self
            .entries
            .values()
            .filter(|entry| distance_sq(entry.item.position, position) <= VIEW_RANGE_SQ)
            .map(|entry| entry.item)
            .collect();
        items.sort_by(|a, b| {
            distance_sq(a.position, position)
                .total_cmp(&distance_sq(b.position, position))
                .then(a.id.cmp(&b.id))
        });
        items.truncate(256);
        items
    }
    pub(super) fn pickup_candidates(&self, position: [f32; 3]) -> Vec<DroppedItem> {
        let mut items: Vec<_> = self
            .entries
            .values()
            .filter(|entry| {
                entry.created.elapsed() >= entry.pickup_delay
                    && distance_sq(entry.item.position, position) <= PICKUP_RANGE_SQ
            })
            .map(|entry| entry.item)
            .collect();
        items.sort_by_key(|item| item.id);
        items
    }
    pub(super) fn take(&mut self, id: u64, count: u16) {
        if let Some(entry) = self.entries.get_mut(&id) {
            if count >= entry.item.count {
                self.entries.remove(&id);
            } else {
                entry.item.count -= count;
            }
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub(super) fn expire(&mut self) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| entry.created.elapsed() < LIFETIME);
        if self.entries.len() != before {
            self.revision = self.revision.wrapping_add(1);
            true
        } else {
            false
        }
    }
    pub(super) fn has_expired(&mut self) -> bool {
        if self.last_gc.elapsed() < Duration::from_secs(1) {
            return false;
        }
        self.last_gc = Instant::now();
        self.entries
            .values()
            .any(|entry| entry.created.elapsed() >= LIFETIME)
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_survive_restart_and_are_still_collectible() {
        let root = std::env::temp_dir().join(format!(
            "bloxgloom-drops-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let mut drops = Drops::open(&root).unwrap();
        drops.spawn([1.0, 2.0, 3.0], 2, 100, Duration::ZERO);
        drops.spawn([1.0, 2.0, 3.0], 2, 50, Duration::ZERO);
        assert_eq!(drops.entries.len(), 2);
        drops.save().unwrap();
        let mut loaded = Drops::open(&root).unwrap();
        assert_eq!(
            loaded
                .entries
                .values()
                .map(|entry| entry.item.count)
                .sum::<u16>(),
            150
        );
        let item = loaded.pickup_candidates([1.0, 2.0, 3.0])[0];
        loaded.take(item.id, item.count);
        loaded.save().unwrap();
        assert_eq!(Drops::open(&root).unwrap().entries.len(), 1);
        let mut bytes = fs::read(root.join("drops.bin")).unwrap();
        bytes[HEADER] ^= 1;
        fs::write(root.join("drops.bin"), bytes).unwrap();
        assert!(Drops::open(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

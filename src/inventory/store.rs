//! Checksummed, atomically replaced per-profile inventory files.
use super::{Inventory, SLOTS, Stack};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
mod tests;

const MAGIC: &[u8; 4] = b"BGIN";
// BGIN v2: magic[4], format u16, revision u64, 36 × (item ID u32,
// count u16, component version u16, component length u16, component bytes),
// FNV-1a checksum u32. All-zero fixed fields mark an empty slot.
const VERSION: u16 = 2;
const HEADER_LEN: usize = 4 + 2 + 8;
const SLOT_FIXED_LEN: usize = 4 + 2 + 2 + 2;
const MIN_LEN: usize = HEADER_LEN + SLOTS * SLOT_FIXED_LEN + 4;
const MAX_LEN: usize = MIN_LEN + SLOTS * super::MAX_COMPONENT_BYTES;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct InventoryStore {
    root: PathBuf,
    catalog: Arc<crate::content::Catalog>,
}

impl InventoryStore {
    pub fn new(world_dir: impl AsRef<Path>) -> io::Result<Self> {
        Self::with_catalog(world_dir, Arc::new(crate::content::catalog().clone()))
    }

    pub fn with_catalog(
        world_dir: impl AsRef<Path>,
        catalog: Arc<crate::content::Catalog>,
    ) -> io::Result<Self> {
        catalog
            .validate()
            .map_err(|_| invalid("incomplete content catalog"))?;
        let root = world_dir.as_ref().join("players");
        fs::create_dir_all(&root)?;
        Ok(Self { root, catalog })
    }

    fn path(&self, profile: u128) -> PathBuf {
        self.root.join(format!("{profile:032x}.inv"))
    }

    pub fn load(&self, profile: u128) -> io::Result<Inventory> {
        if profile == 0 {
            return Err(invalid("missing player profile"));
        }
        let Some(bytes) = self.read_snapshot(profile)? else {
            return Ok(Inventory::default());
        };
        Self::decode_snapshot_with_catalog(&bytes, &self.catalog)
    }

    /// Reads exact BGIN v2 bytes.
    pub fn read_snapshot(&self, profile: u128) -> io::Result<Option<Vec<u8>>> {
        if profile == 0 {
            return Err(invalid("missing player profile"));
        }
        match File::open(self.path(profile)) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take((MAX_LEN + 1) as u64).read_to_end(&mut bytes)?;
                if bytes.len() > MAX_LEN {
                    return Err(invalid("inventory file too large"));
                }
                Ok(Some(bytes))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Encodes an inventory using BGIN v2 bytes.
    pub fn encode_snapshot(inventory: &Inventory) -> io::Result<Vec<u8>> {
        Self::encode_snapshot_with_catalog(inventory, crate::content::catalog())
    }

    pub fn encode_snapshot_with_catalog(
        inventory: &Inventory,
        catalog: &crate::content::Catalog,
    ) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(MIN_LEN);
        bytes.extend(MAGIC);
        bytes.extend(VERSION.to_le_bytes());
        bytes.extend(inventory.revision.to_le_bytes());
        for slot in &inventory.slots {
            if let Some(stack) = slot {
                if !stack.valid_in(catalog) {
                    return Err(invalid("invalid inventory stack"));
                }
                bytes.extend(stack.item.0.to_le_bytes());
                bytes.extend(stack.count.to_le_bytes());
                if let Some(payload) = &stack.components {
                    bytes.extend(payload.version.to_le_bytes());
                    bytes.extend((payload.bytes.len() as u16).to_le_bytes());
                    bytes.extend(&payload.bytes);
                } else {
                    bytes.extend([0; 4]);
                }
            } else {
                bytes.extend([0; SLOT_FIXED_LEN]);
            }
        }
        bytes.extend(checksum(&bytes).to_le_bytes());
        Ok(bytes)
    }

    /// Decodes and validates exact BGIN v2 bytes.
    pub fn decode_snapshot_with_catalog(
        bytes: &[u8],
        catalog: &crate::content::Catalog,
    ) -> io::Result<Inventory> {
        if !(MIN_LEN..=MAX_LEN).contains(&bytes.len())
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != VERSION
        {
            return Err(invalid("invalid inventory file"));
        }
        let checksum_at = bytes.len() - 4;
        if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
            != checksum(&bytes[..checksum_at])
        {
            return Err(invalid("inventory checksum mismatch"));
        }
        let mut inventory = Inventory {
            revision: u64::from_le_bytes(bytes[6..14].try_into().unwrap()),
            ..Inventory::default()
        };
        let mut offset = HEADER_LEN;
        for slot in &mut inventory.slots {
            let end = offset
                .checked_add(SLOT_FIXED_LEN)
                .ok_or_else(|| invalid("invalid inventory length"))?;
            let entry = bytes
                .get(offset..end)
                .filter(|_| end <= checksum_at)
                .ok_or_else(|| invalid("truncated inventory slot"))?;
            let item = crate::items::ItemId(u32::from_le_bytes(entry[..4].try_into().unwrap()));
            let count = u16::from_le_bytes(entry[4..6].try_into().unwrap());
            let component_version = u16::from_le_bytes(entry[6..8].try_into().unwrap());
            let component_len = u16::from_le_bytes(entry[8..10].try_into().unwrap()) as usize;
            offset = end;
            if component_len > super::MAX_COMPONENT_BYTES || offset + component_len > checksum_at {
                return Err(invalid("invalid inventory components"));
            }
            if item.0 == 0 {
                if count != 0 || component_version != 0 || component_len != 0 {
                    return Err(invalid("invalid empty inventory slot"));
                }
            } else {
                let stack = if component_len == 0 {
                    if component_version != 0 {
                        return Err(invalid("invalid inventory component version"));
                    }
                    Stack::new(item, count)
                } else {
                    Stack::with_components(
                        item,
                        count,
                        component_version,
                        bytes[offset..offset + component_len].to_vec(),
                    )
                    .ok_or_else(|| invalid("invalid inventory components"))?
                };
                if !stack.valid_in(catalog) {
                    return Err(invalid("invalid inventory stack"));
                }
                *slot = Some(stack);
            }
            offset += component_len;
        }
        if offset != checksum_at {
            return Err(invalid("trailing inventory bytes"));
        }
        Ok(inventory)
    }

    #[cfg(test)]
    pub fn save(&self, profile: u128, inventory: &Inventory) -> io::Result<()> {
        if profile == 0 {
            return Err(invalid("missing player profile"));
        }
        let bytes = Self::encode_snapshot_with_catalog(inventory, &self.catalog)?;
        self.checkpoint_snapshot(profile, &bytes)
    }

    /// Validates and atomically checkpoints exact BGIN bytes from a committed
    /// journal value. Existing malformed snapshots are deliberately not read
    /// or overwritten here; startup validates them before replay.
    pub fn checkpoint_snapshot(&self, profile: u128, bytes: &[u8]) -> io::Result<()> {
        if profile == 0 {
            return Err(invalid("missing player profile"));
        }
        Self::decode_snapshot_with_catalog(bytes, &self.catalog)?;
        let destination = self.path(profile);
        let temporary = self.root.join(format!(
            ".{profile:032x}.{}.{}.tmp",
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
            fs::rename(&temporary, destination)?;
            File::open(&self.root)?.sync_all()
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
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

//! Checksummed, atomically replaced per-profile inventory files.
use super::{Inventory, SLOTS, Stack};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAGIC: &[u8; 4] = b"BGIN";
const VERSION: u16 = 1;
const LEN: usize = 4 + 2 + 8 + SLOTS * 3 + 4;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct InventoryStore {
    root: PathBuf,
}

impl InventoryStore {
    pub fn new(world_dir: impl AsRef<Path>) -> io::Result<Self> {
        let root = world_dir.as_ref().join("players");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path(&self, profile: u128) -> PathBuf {
        self.root.join(format!("{profile:032x}.inv"))
    }

    pub fn load(&self, profile: u128) -> io::Result<Inventory> {
        if profile == 0 {
            return Err(invalid("missing player profile"));
        }
        let bytes = match fs::read(self.path(profile)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Inventory::default());
            }
            Err(error) => return Err(error),
        };
        if bytes.len() != LEN
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != VERSION
        {
            return Err(invalid("invalid inventory file"));
        }
        let checksum_at = LEN - 4;
        if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
            != checksum(&bytes[..checksum_at])
        {
            return Err(invalid("inventory checksum mismatch"));
        }
        let mut inventory = Inventory {
            revision: u64::from_le_bytes(bytes[6..14].try_into().unwrap()),
            ..Inventory::default()
        };
        for (index, entry) in bytes[14..checksum_at].chunks_exact(3).enumerate() {
            let (block, count) = (entry[0], u16::from_le_bytes([entry[1], entry[2]]));
            if block != 0 || count != 0 {
                let stack = Stack { block, count };
                if !stack.valid() {
                    return Err(invalid("invalid inventory stack"));
                }
                inventory.slots[index] = Some(stack);
            }
        }
        Ok(inventory)
    }

    pub fn save(&self, profile: u128, inventory: &Inventory) -> io::Result<()> {
        if profile == 0 {
            return Err(invalid("missing player profile"));
        }
        let mut bytes = Vec::with_capacity(LEN);
        bytes.extend(MAGIC);
        bytes.extend(VERSION.to_le_bytes());
        bytes.extend(inventory.revision.to_le_bytes());
        for slot in inventory.slots {
            if let Some(stack) = slot {
                if !stack.valid() {
                    return Err(invalid("invalid inventory stack"));
                }
                bytes.push(stack.block);
                bytes.extend(stack.count.to_le_bytes());
            } else {
                bytes.extend([0, 0, 0]);
            }
        }
        bytes.extend(checksum(&bytes).to_le_bytes());
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
            file.write_all(&bytes)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_survives_restart_and_corruption_is_rejected() {
        let root = std::env::temp_dir().join(format!(
            "bloxgloom-inventory-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let store = InventoryStore::new(&root).unwrap();
        let mut inventory = Inventory::default();
        inventory.insert(3, 129);
        store.save(42, &inventory).unwrap();
        assert_eq!(store.load(42).unwrap(), inventory);
        let path = store.path(42);
        let mut bytes = fs::read(&path).unwrap();
        bytes[20] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(store.load(42).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

//! Checksummed, atomically replaced per-profile inventory files.
use super::{Inventory, SLOTS, Stack};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
mod tests;

const MAGIC: &[u8; 4] = b"BGIN";
const VERSION: u16 = 1;
const LEN: usize = 4 + 2 + 8 + SLOTS * 3 + 4;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
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
        let Some(bytes) = self.read_snapshot(profile)? else {
            return Ok(Inventory::default());
        };
        Self::decode_snapshot(&bytes)
    }

    /// Reads exact BGIN bytes, preserving the existing versioned save format.
    pub fn read_snapshot(&self, profile: u128) -> io::Result<Option<Vec<u8>>> {
        if profile == 0 {
            return Err(invalid("missing player profile"));
        }
        match fs::read(self.path(profile)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Encodes an inventory using the established BGIN v1 bytes.
    pub fn encode_snapshot(inventory: &Inventory) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(LEN);
        bytes.extend(MAGIC);
        bytes.extend(VERSION.to_le_bytes());
        bytes.extend(inventory.revision.to_le_bytes());
        for slot in inventory.slots {
            if let Some(stack) = slot {
                if !stack.valid() {
                    return Err(invalid("invalid inventory stack"));
                }
                bytes.push(stack.item);
                bytes.extend(stack.count.to_le_bytes());
            } else {
                bytes.extend([0, 0, 0]);
            }
        }
        bytes.extend(checksum(&bytes).to_le_bytes());
        Ok(bytes)
    }

    /// Decodes and validates exact BGIN v1 bytes.
    pub fn decode_snapshot(bytes: &[u8]) -> io::Result<Inventory> {
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
            let (item, count) = (entry[0], u16::from_le_bytes([entry[1], entry[2]]));
            if item != 0 || count != 0 {
                let stack = Stack { item, count };
                if !stack.valid() {
                    return Err(invalid("invalid inventory stack"));
                }
                inventory.slots[index] = Some(stack);
            }
        }
        Ok(inventory)
    }

    #[cfg(test)]
    pub fn save(&self, profile: u128, inventory: &Inventory) -> io::Result<()> {
        if profile == 0 {
            return Err(invalid("missing player profile"));
        }
        let bytes = Self::encode_snapshot(inventory)?;
        self.checkpoint_snapshot(profile, &bytes)
    }

    /// Validates and atomically checkpoints exact BGIN bytes from a committed
    /// journal value. Existing malformed snapshots are deliberately not read
    /// or overwritten here; startup validates them before replay.
    pub fn checkpoint_snapshot(&self, profile: u128, bytes: &[u8]) -> io::Result<()> {
        if profile == 0 {
            return Err(invalid("missing player profile"));
        }
        Self::decode_snapshot(bytes)?;
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

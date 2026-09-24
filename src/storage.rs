//! Durable, sparse overrides of procedurally generated chunks.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::world::{BlockId, CHUNK_VOLUME, ChunkKey, MAX_BLOCK, TERRAIN_GENERATOR_VERSION};

const MAGIC: &[u8; 4] = b"BGED";
const FORMAT_VERSION: u16 = 2;
const HEADER_LEN: usize = 4 + 2 + 2 + 8 + 8 + 2;
const WORLD_MAGIC: &[u8; 4] = b"BGWD";
const WORLD_META: &str = "world.meta";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default)]
pub struct SavedEdits {
    pub version: u64,
    pub blocks: BTreeMap<u16, BlockId>,
}

#[derive(Debug)]
pub struct Storage {
    root: PathBuf,
    seed: u64,
}

impl Storage {
    pub fn new(root: impl AsRef<Path>, seed: u64) -> io::Result<Self> {
        fs::create_dir_all(root.as_ref())?;
        let root = root.as_ref().to_owned();
        let metadata_path = root.join(WORLD_META);
        match fs::read(&metadata_path) {
            Ok(bytes) => {
                if bytes.len() != 14 || &bytes[..4] != WORLD_MAGIC {
                    return Err(invalid_data("invalid world metadata"));
                }
                let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
                if version != TERRAIN_GENERATOR_VERSION {
                    return Err(invalid_data("incompatible terrain generator version"));
                }
                let saved_seed = u64::from_le_bytes(bytes[6..14].try_into().unwrap());
                if saved_seed != seed {
                    return Err(invalid_data("world directory belongs to another seed"));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                for entry in fs::read_dir(&root)? {
                    let entry = entry?;
                    if entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "bged")
                    {
                        return Err(invalid_data(
                            "legacy world edits require a compatible terrain generator",
                        ));
                    }
                }
                let mut bytes = Vec::with_capacity(14);
                bytes.extend_from_slice(WORLD_MAGIC);
                bytes.extend_from_slice(&TERRAIN_GENERATOR_VERSION.to_le_bytes());
                bytes.extend_from_slice(&seed.to_le_bytes());
                let temp_id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
                let temp_path = root.join(format!(
                    ".world-meta.{}.{}.tmp",
                    std::process::id(),
                    temp_id
                ));
                let result = (|| {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&temp_path)?;
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    drop(file);
                    fs::rename(&temp_path, &metadata_path)?;
                    File::open(&root)?.sync_all()
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temp_path);
                }
                result?;
            }
            Err(error) => return Err(error),
        }
        Ok(Self { root, seed })
    }

    fn path(&self, key: ChunkKey) -> PathBuf {
        self.root
            .join(format!("{}_{}_{}.bged", key.x, key.y, key.z))
    }

    pub fn load(&self, key: ChunkKey) -> io::Result<SavedEdits> {
        let bytes = match fs::read(self.path(key)) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(SavedEdits::default()),
            Err(err) => return Err(err),
        };
        if bytes.len() < 6 || &bytes[..4] != MAGIC {
            return Err(invalid_data("invalid chunk edits header"));
        }
        let format = u16::from_le_bytes([bytes[4], bytes[5]]);
        if format != FORMAT_VERSION {
            return Err(invalid_data(
                "incompatible chunk edits format or terrain generator",
            ));
        }
        if bytes.len() < HEADER_LEN + 4 {
            return Err(invalid_data("invalid chunk edits header"));
        }
        let generator_version = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        if generator_version != TERRAIN_GENERATOR_VERSION {
            return Err(invalid_data("incompatible terrain generator version"));
        }
        let seed = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        if seed != self.seed {
            return Err(invalid_data("saved edits belong to another world seed"));
        }
        let version = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        let count = u16::from_le_bytes(bytes[24..26].try_into().unwrap()) as usize;
        if count > CHUNK_VOLUME || bytes.len() != HEADER_LEN + count * 3 + 4 {
            return Err(invalid_data("invalid chunk edits length"));
        }
        let checksum_offset = bytes.len() - 4;
        let stored_checksum = u32::from_le_bytes(bytes[checksum_offset..].try_into().unwrap());
        if stored_checksum != checksum(&bytes[..checksum_offset]) {
            return Err(invalid_data("chunk edits checksum mismatch"));
        }
        let mut blocks = BTreeMap::new();
        for record in bytes[HEADER_LEN..checksum_offset].chunks_exact(3) {
            let index = u16::from_le_bytes([record[0], record[1]]);
            if index as usize >= CHUNK_VOLUME
                || record[2] > MAX_BLOCK
                || blocks.insert(index, record[2]).is_some()
            {
                return Err(invalid_data("invalid or duplicate chunk edit index"));
            }
        }
        Ok(SavedEdits { version, blocks })
    }

    pub fn save(&self, key: ChunkKey, edits: &SavedEdits) -> io::Result<()> {
        if edits.blocks.len() > CHUNK_VOLUME {
            return Err(invalid_data("too many chunk edits"));
        }
        let mut bytes = Vec::with_capacity(HEADER_LEN + edits.blocks.len() * 3 + 4);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&TERRAIN_GENERATOR_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.seed.to_le_bytes());
        bytes.extend_from_slice(&edits.version.to_le_bytes());
        bytes.extend_from_slice(&(edits.blocks.len() as u16).to_le_bytes());
        for (&index, &block) in &edits.blocks {
            if index as usize >= CHUNK_VOLUME {
                return Err(invalid_data("invalid chunk edit index"));
            }
            bytes.extend_from_slice(&index.to_le_bytes());
            bytes.push(block);
        }
        let checksum = checksum(&bytes);
        bytes.extend_from_slice(&checksum.to_le_bytes());

        let final_path = self.path(key);
        let temp_id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = self.root.join(format!(
            ".{}_{}_{}.{}.{}.tmp",
            key.x,
            key.y,
            key.z,
            std::process::id(),
            temp_id
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp_path, &final_path)?;
            File::open(&self.root)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

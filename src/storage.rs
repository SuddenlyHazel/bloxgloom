//! Durable, sparse overrides of procedurally generated chunks.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::content::{Catalog, ContentManifest, MAX_MANIFEST_BYTES};
use crate::world::{
    BlockId, CHUNK_VOLUME, ChunkKey, Generator, MAX_GENERATION_IDENTITY_BYTES,
    TERRAIN_GENERATOR_VERSION,
};

const MAGIC: &[u8; 4] = b"BGED";
// BGED v3: magic[4], format u16, terrain generator u16, seed u64,
// revision u64, count u16, sorted `(cell_index u16, state_id u32)` records,
// FNV-1a checksum u32. Older formats are rejected, never upgraded in place.
const FORMAT_VERSION: u16 = 3;
const HEADER_LEN: usize = 4 + 2 + 2 + 8 + 8 + 2;
const MAX_SNAPSHOT_BYTES: usize = HEADER_LEN + CHUNK_VOLUME * 6 + 4;
const WORLD_MAGIC: &[u8; 4] = b"BGWD";
const WORLD_META: &str = "world.meta";
const CONTENT_MAP: &str = "content.map";
pub(crate) const WORLD_LOCK: &str = ".world.lock";
pub(crate) const CONVERSION_INCOMPLETE: &str = ".conversion-incomplete";
const SAVE_FORMAT_VERSION: u16 = 7;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedEdits {
    pub version: u64,
    pub blocks: BTreeMap<u16, BlockId>,
}

#[derive(Clone, Debug)]
pub struct Storage {
    root: PathBuf,
    seed: u64,
    catalog: Arc<Catalog>,
    // Clones share one open lock descriptor; a second writer is excluded for
    // the entire world lifetime.
    _world_lock: Arc<File>,
}

impl Storage {
    #[cfg(test)]
    pub fn new(root: impl AsRef<Path>, seed: u64) -> io::Result<Self> {
        Self::with_catalog(root, seed, Arc::new(crate::content::catalog().clone()))
    }

    #[cfg(test)]
    pub fn with_catalog(
        root: impl AsRef<Path>,
        seed: u64,
        catalog: Arc<Catalog>,
    ) -> io::Result<Self> {
        Self::with_generation(root, seed, catalog, &Generator::default())
    }

    pub(crate) fn with_generation(
        root: impl AsRef<Path>,
        seed: u64,
        catalog: Arc<Catalog>,
        generator: &Generator,
    ) -> io::Result<Self> {
        let identity = generator.identity();
        catalog
            .validate()
            .map_err(|_| invalid_data("incomplete content catalog"))?;
        let root = root.as_ref().to_owned();
        if root
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".partial"))
        {
            return Err(invalid_data(
                "incomplete conversion stage cannot be opened as a world",
            ));
        }
        let metadata_path = root.join(WORLD_META);
        // Reject an incompatible existing world before creating even the lock
        // file. Re-read under the lock below to guard against concurrent edits.
        match read_world_metadata(&metadata_path, &identity) {
            Ok(saved_seed) if saved_seed != seed => {
                return Err(invalid_data("world directory belongs to another seed"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                verify_new_world_directory(&root)?;
            }
            Err(error) => return Err(error),
        }
        fs::create_dir_all(&root)?;
        if root.join(CONVERSION_INCOMPLETE).exists() {
            return Err(invalid_data("world conversion is incomplete"));
        }
        let world_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(WORLD_LOCK))?;
        world_lock.try_lock().map_err(|error| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("world is already open by another writer: {error}"),
            )
        })?;
        let catalog = match read_world_metadata(&metadata_path, &identity) {
            Ok(saved_seed) => {
                if saved_seed != seed {
                    return Err(invalid_data("world directory belongs to another seed"));
                }
                resolve_content_map_with(&root, false, &catalog)?
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                verify_new_world_directory(&root)?;
                let resolved = resolve_content_map_with(&root, true, &catalog)?;
                let mut bytes = Vec::with_capacity(16 + identity.len());
                bytes.extend_from_slice(WORLD_MAGIC);
                bytes.extend_from_slice(&SAVE_FORMAT_VERSION.to_le_bytes());
                bytes.extend_from_slice(&TERRAIN_GENERATOR_VERSION.to_le_bytes());
                bytes.extend_from_slice(&seed.to_le_bytes());
                bytes.extend_from_slice(&identity);
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
                resolved
            }
            Err(error) => return Err(error),
        };
        Ok(Self {
            root,
            seed,
            catalog,
            _world_lock: Arc::new(world_lock),
        })
    }

    pub fn catalog_arc(&self) -> Arc<Catalog> {
        Arc::clone(&self.catalog)
    }

    fn path(&self, key: ChunkKey) -> PathBuf {
        self.root
            .join(format!("{}_{}_{}.bged", key.x, key.y, key.z))
    }

    pub fn load(&self, key: ChunkKey) -> io::Result<SavedEdits> {
        let bytes = self.read_snapshot_bytes(key)?;
        self.decode_snapshot(bytes.as_deref())
    }

    /// Reads the exact persisted BGED snapshot. `None` means no override file.
    pub fn read_snapshot(&self, key: ChunkKey) -> io::Result<Option<Vec<u8>>> {
        let bytes = self.read_snapshot_bytes(key)?;
        self.decode_snapshot(bytes.as_deref())?;
        Ok(bytes)
    }

    fn read_snapshot_bytes(&self, key: ChunkKey) -> io::Result<Option<Vec<u8>>> {
        match File::open(self.path(key)) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take((MAX_SNAPSHOT_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > MAX_SNAPSHOT_BYTES {
                    return Err(invalid_data("chunk edits file too large"));
                }
                Ok(Some(bytes))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Encodes sparse edits using the BGED v3 format. A pristine chunk
    /// has no persisted override file and is represented as `None`.
    pub fn encode_snapshot(&self, edits: &SavedEdits) -> io::Result<Option<Vec<u8>>> {
        if edits.blocks.len() > CHUNK_VOLUME {
            return Err(invalid_data("too many chunk edits"));
        }
        if edits.version == 0 && edits.blocks.is_empty() {
            return Ok(None);
        }

        let mut bytes = Vec::with_capacity(HEADER_LEN + edits.blocks.len() * 6 + 4);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&TERRAIN_GENERATOR_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.seed.to_le_bytes());
        bytes.extend_from_slice(&edits.version.to_le_bytes());
        bytes.extend_from_slice(&(edits.blocks.len() as u16).to_le_bytes());
        for (&index, &block) in &edits.blocks {
            if index as usize >= CHUNK_VOLUME || self.catalog.state(block).is_none() {
                return Err(invalid_data("invalid chunk edit"));
            }
            bytes.extend_from_slice(&index.to_le_bytes());
            bytes.extend_from_slice(&block.0.to_le_bytes());
        }
        let checksum = checksum(&bytes);
        bytes.extend_from_slice(&checksum.to_le_bytes());
        Ok(Some(bytes))
    }

    /// Decodes a validated BGED snapshot. `None` is the canonical pristine
    /// state; a present but empty byte string is corrupt data.
    pub fn decode_snapshot(&self, bytes: Option<&[u8]>) -> io::Result<SavedEdits> {
        let Some(bytes) = bytes else {
            return Ok(SavedEdits::default());
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
        if count > CHUNK_VOLUME || bytes.len() != HEADER_LEN + count * 6 + 4 {
            return Err(invalid_data("invalid chunk edits length"));
        }
        let checksum_offset = bytes.len() - 4;
        let stored_checksum = u32::from_le_bytes(bytes[checksum_offset..].try_into().unwrap());
        if stored_checksum != checksum(&bytes[..checksum_offset]) {
            return Err(invalid_data("chunk edits checksum mismatch"));
        }
        let mut blocks = BTreeMap::new();
        let mut previous = None;
        for record in bytes[HEADER_LEN..checksum_offset].chunks_exact(6) {
            let index = u16::from_le_bytes([record[0], record[1]]);
            let state =
                crate::content::BlockStateId(u32::from_le_bytes(record[2..6].try_into().unwrap()));
            if index as usize >= CHUNK_VOLUME
                || previous.is_some_and(|old| index <= old)
                || self.catalog.state(state).is_none()
                || blocks.insert(index, state).is_some()
            {
                return Err(invalid_data("invalid or duplicate chunk edit index"));
            }
            previous = Some(index);
        }
        Ok(SavedEdits { version, blocks })
    }

    #[cfg(test)]
    pub fn save(&self, key: ChunkKey, edits: &SavedEdits) -> io::Result<()> {
        let snapshot = self.encode_snapshot(edits)?;
        self.replace_snapshot(key, snapshot.as_deref())
    }

    /// Atomically installs an exact BGED snapshot. `None` removes the override
    /// file and restores the generated baseline. Present bytes are validated
    /// before replacing the durable file.
    pub fn replace_snapshot(&self, key: ChunkKey, snapshot: Option<&[u8]>) -> io::Result<()> {
        if let Some(snapshot) = snapshot {
            self.decode_snapshot(Some(snapshot))?;
        } else {
            let path = self.path(key);
            match fs::remove_file(path) {
                Ok(()) => File::open(&self.root)?.sync_all()?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            return Ok(());
        }

        let snapshot = snapshot.expect("validated snapshot");
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
            file.write_all(snapshot)?;
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

/// Without metadata no existing override or WAL can be attributed to this
/// generator. An interrupted first creation also fails closed; never adopt it.
fn verify_new_world_directory(root: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        if entry?.file_name() != WORLD_LOCK {
            return Err(invalid_data(
                "world metadata missing from nonempty directory; no automatic upgrade",
            ));
        }
    }
    Ok(())
}

fn read_world_metadata(path: &Path, identity: &[u8]) -> io::Result<u64> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((16 + MAX_GENERATION_IDENTITY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() == 14 && &bytes[..4] == WORLD_MAGIC {
        return Err(invalid_data(
            "unsupported old save format; no automatic upgrade",
        ));
    }
    if bytes.len() < 16 || &bytes[..4] != WORLD_MAGIC {
        return Err(invalid_data("invalid world metadata"));
    }
    if u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != SAVE_FORMAT_VERSION {
        return Err(invalid_data(
            "unsupported save format; no automatic upgrade",
        ));
    }
    if u16::from_le_bytes(bytes[6..8].try_into().unwrap()) != TERRAIN_GENERATOR_VERSION {
        return Err(invalid_data("incompatible terrain generator version"));
    }
    if &bytes[16..] != identity {
        return Err(invalid_data(
            "incompatible generation contributors or revisions",
        ));
    }
    Ok(u64::from_le_bytes(bytes[8..16].try_into().unwrap()))
}

#[cfg(test)]
fn verify_content_map_with(
    root: &Path,
    new_world: bool,
    catalog: &crate::content::Catalog,
) -> io::Result<()> {
    resolve_content_map_with(root, new_world, catalog).map(|_| ())
}

fn resolve_content_map_with(
    root: &Path,
    new_world: bool,
    catalog: &Catalog,
) -> io::Result<Arc<Catalog>> {
    let path = root.join(CONTENT_MAP);
    let current = ContentManifest::from_catalog(catalog);
    match File::open(&path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take((MAX_MANIFEST_BYTES + 1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_MANIFEST_BYTES {
                return Err(invalid_data("content manifest too large"));
            }
            let mut saved = ContentManifest::decode(&bytes)?;
            let (resolved, changed) = saved.resolve_world_catalog(catalog)?;
            if changed {
                write_content_map(root, &saved)?;
            }
            Ok(Arc::new(resolved))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if !new_world {
                return Err(invalid_data("world is missing content manifest"));
            }
            write_content_map(root, &current)?;
            Ok(Arc::new(catalog.clone()))
        }
        Err(error) => Err(error),
    }
}

fn write_content_map(root: &Path, manifest: &ContentManifest) -> io::Result<()> {
    let bytes = manifest.encode()?;
    let temporary = root.join(format!(
        ".content-map.{}.{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, root.join(CONTENT_MAP))?;
        File::open(root)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
#[path = "storage/tests.rs"]
mod tests;

//! Atomic persistence for the entity aggregate checkpoint.
//!
//! The journal remains authoritative for entity mutations. This file is a
//! compact checkpoint of the aggregate index/store and is replaced only after
//! the caller has a committed snapshot to publish.

use super::persistence::{MAX_ENTITY_SNAPSHOT_BYTES, validate_checkpoint_frame};
use super::types::EntityError;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

const DIRECTORY_NAME: &str = "entities";
const CHECKPOINT_NAME: &str = "entities.bin";
const TEMPORARY_NAME: &str = ".entities.bin.tmp";

/// Bounded, atomic file access for the entity checkpoint.
///
/// `root` is the world save directory. A crash-left temporary file is treated
/// as an ambiguous checkpoint publication and causes reads/writes to fail
/// closed; recovery must not guess whether the old or new frontier won.
#[derive(Clone, Debug)]
pub(in crate::server) struct EntityCheckpointStore {
    directory: PathBuf,
    checkpoint: PathBuf,
    temporary: PathBuf,
}

impl EntityCheckpointStore {
    pub(in crate::server) fn new(root: &Path) -> io::Result<Self> {
        let directory = root.join(DIRECTORY_NAME);
        fs::create_dir_all(&directory)?;
        Ok(Self {
            checkpoint: directory.join(CHECKPOINT_NAME),
            temporary: directory.join(TEMPORARY_NAME),
            directory,
        })
    }

    /// Read and envelope-validate the bounded BGEN checkpoint. Type-specific
    /// payload validation is performed by `decode_checkpoint` at the caller.
    pub(in crate::server) fn read(&self) -> io::Result<Option<Vec<u8>>> {
        self.reject_interrupted_temporary()?;
        let metadata = match fs::symlink_metadata(&self.checkpoint) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if !metadata.file_type().is_file() {
            return Err(invalid_data("entity checkpoint is not a regular file"));
        }
        let file = File::open(&self.checkpoint)?;
        let mut bytes = Vec::new();
        file.take((MAX_ENTITY_SNAPSHOT_BYTES as u64).saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_ENTITY_SNAPSHOT_BYTES {
            return Err(invalid_data("entity checkpoint exceeds size limit"));
        }
        validate_frame(&bytes)?;
        Ok(Some(bytes))
    }

    /// Atomically publish a fully encoded BGEN checkpoint.
    pub(in crate::server) fn write(&self, bytes: &[u8]) -> io::Result<()> {
        validate_frame(bytes)?;
        self.reject_interrupted_temporary()?;

        let mut created_temporary = false;
        let mut renamed = false;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&self.temporary)?;
            created_temporary = true;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&self.temporary, &self.checkpoint)?;
            renamed = true;
            File::open(&self.directory)?.sync_all()
        })();

        if result.is_err() && created_temporary && !renamed {
            // A failed in-process write is known to be ours and can be cleaned
            // safely. A process-crash leftover is instead rejected on startup.
            let _ = fs::remove_file(&self.temporary);
            let _ = File::open(&self.directory).and_then(|directory| directory.sync_all());
        }
        result
    }

    fn reject_interrupted_temporary(&self) -> io::Result<()> {
        match fs::symlink_metadata(&self.temporary) {
            Ok(_) => Err(invalid_data(
                "interrupted entity checkpoint publication requires recovery",
            )),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn validate_frame(bytes: &[u8]) -> io::Result<()> {
    validate_checkpoint_frame(bytes).map_err(entity_invalid_data)
}

fn entity_invalid_data(error: EntityError) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, error)
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::entities::codec::Encoder;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn unique_directory() -> PathBuf {
        let id = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "bloxgloom-entity-checkpoint-{}-{id}",
            std::process::id()
        ))
    }

    fn valid_empty_checkpoint() -> Vec<u8> {
        let mut encoder = Encoder::with_capacity(38);
        encoder.raw(b"BGEN");
        encoder.u16(2);
        encoder.u64(1);
        encoder.u64(0);
        encoder.u32(0);
        encoder.u32(0);
        encoder.u32(0);
        encoder.finish_crc().unwrap()
    }

    #[test]
    fn checkpoint_write_read_is_atomic_and_bounded() {
        let directory = unique_directory();
        let checkpoint = EntityCheckpointStore::new(&directory).unwrap();
        let value = valid_empty_checkpoint();
        assert_eq!(checkpoint.read().unwrap(), None);
        checkpoint.write(&value).unwrap();
        assert_eq!(checkpoint.read().unwrap(), Some(value));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn checkpoint_rejects_bad_magic_version_and_checksum() {
        let directory = unique_directory();
        let checkpoint = EntityCheckpointStore::new(&directory).unwrap();
        let mut bad_magic = valid_empty_checkpoint();
        bad_magic[0] = b'X';
        assert_eq!(
            checkpoint.write(&bad_magic).unwrap_err().kind(),
            ErrorKind::InvalidData
        );

        let mut bad_version = valid_empty_checkpoint();
        bad_version[4..6].copy_from_slice(&99u16.to_le_bytes());
        let checksum = crate::server::entities::codec::crc32(&bad_version[..bad_version.len() - 4]);
        let end = bad_version.len();
        bad_version[end - 4..].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            checkpoint.write(&bad_version).unwrap_err().kind(),
            ErrorKind::InvalidData
        );

        let mut bad_checksum = valid_empty_checkpoint();
        let end = bad_checksum.len();
        bad_checksum[end - 1] ^= 0x80;
        assert_eq!(
            checkpoint.write(&bad_checksum).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn crash_left_temporary_checkpoint_fails_closed() {
        let directory = unique_directory();
        let checkpoint = EntityCheckpointStore::new(&directory).unwrap();
        fs::write(&checkpoint.temporary, b"partial").unwrap();
        assert_eq!(
            checkpoint.read().unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(
            checkpoint
                .write(&valid_empty_checkpoint())
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidData
        );
        fs::remove_dir_all(directory).unwrap();
    }
}

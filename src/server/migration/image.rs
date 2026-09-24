//! Closed-source capture and change detection. The legacy binary does not
//! participate in locking, so the operator must stop it before conversion.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

static STAGE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    modified: Option<SystemTime>,
    device: u64,
    inode: u64,
    checksum: u64,
}

pub(super) struct SourceImage {
    source: PathBuf,
    destination: PathBuf,
    files: BTreeMap<PathBuf, Fingerprint>,
    _lock: File,
}

impl SourceImage {
    pub(super) fn capture(source: &Path, destination: &Path) -> io::Result<Self> {
        let source = source.canonicalize()?;
        if !source.is_dir() {
            return Err(invalid("legacy source must be a directory"));
        }
        let source_parent = source
            .parent()
            .ok_or_else(|| invalid("invalid source path"))?;
        let destination_name = destination
            .file_name()
            .ok_or_else(|| invalid("destination needs a final directory name"))?;
        let destination_parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        let destination = destination_parent.join(destination_name);
        if destination.exists() || destination == source {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination must not already exist",
            ));
        }
        let source_meta = fs::metadata(&source)?;
        let destination_meta = fs::metadata(&destination_parent)?;
        if device(&source_meta) != device(&destination_meta) {
            return Err(invalid("source and destination must be on one filesystem"));
        }
        let source_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("source directory name must be UTF-8"))?;
        let lock_path = source_parent.join(format!(".{source_name}.migration.lock"));
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock.try_lock().map_err(|_| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                "another conversion holds the source lock",
            )
        })?;
        let files = scan(&source)?;
        if !files.contains_key(Path::new("world.meta"))
            || !(files.contains_key(Path::new("server.wal"))
                || files.contains_key(Path::new("server.wal.manifest")))
        {
            return Err(invalid("legacy world metadata or journal is missing"));
        }
        Ok(Self {
            source,
            destination,
            files,
            _lock: lock,
        })
    }

    pub(super) fn destination(&self) -> &Path {
        &self.destination
    }

    pub(super) fn source_file_count(&self) -> usize {
        self.files.len()
    }

    pub(super) fn stage(&self) -> io::Result<PathBuf> {
        let parent = self
            .destination
            .parent()
            .expect("absolute destination has parent");
        let name = self
            .destination
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("destination directory name must be UTF-8"))?;
        for _ in 0..128 {
            let stage = parent.join(format!(
                ".{name}.migration-v4.{}.{}.partial",
                std::process::id(),
                STAGE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&stage) {
                Ok(()) => {
                    File::open(parent)?.sync_all()?;
                    return Ok(stage);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "migration staging names exhausted",
        ))
    }

    pub(super) fn copy_into(&self, stage: &Path) -> io::Result<PathBuf> {
        let copy = stage.join(".legacy-copy");
        fs::create_dir(&copy)?;
        if self.files.keys().any(|path| path.starts_with("players")) {
            fs::create_dir(copy.join("players"))?;
        }
        for (relative, expected) in &self.files {
            let source = self.source.join(relative);
            let target = copy.join(relative);
            fs::copy(&source, &target)?;
            File::open(&target)?.sync_all()?;
            let actual = fingerprint(&target)?;
            if actual.len != expected.len || actual.checksum != expected.checksum {
                return Err(invalid("legacy file changed while being copied"));
            }
        }
        if copy.join("players").exists() {
            File::open(copy.join("players"))?.sync_all()?;
        }
        File::open(&copy)?.sync_all()?;
        self.verify_unchanged()?;
        Ok(copy)
    }

    pub(super) fn verify_unchanged(&self) -> io::Result<()> {
        if scan(&self.source)? != self.files {
            return Err(invalid(
                "legacy source changed during conversion; stop the old server",
            ));
        }
        Ok(())
    }
}

fn scan(root: &Path) -> io::Result<BTreeMap<PathBuf, Fingerprint>> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| invalid("non-UTF-8 legacy path"))?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if name == "players" && meta.is_dir() {
            for player in fs::read_dir(&path)? {
                let player = player?;
                let player_name = player.file_name();
                let player_name = player_name
                    .to_str()
                    .ok_or_else(|| invalid("non-UTF-8 player path"))?;
                let file_meta = fs::symlink_metadata(player.path())?;
                if !file_meta.is_file() || parse_profile(player_name).is_none() {
                    return Err(invalid("unexpected legacy player entry"));
                }
                files.insert(
                    PathBuf::from("players").join(player_name),
                    fingerprint(&player.path())?,
                );
            }
        } else if meta.is_file() && allowed_root_name(name) {
            files.insert(PathBuf::from(name), fingerprint(&path)?);
        } else {
            return Err(invalid(
                "unexpected legacy save entry; refusing silent omission",
            ));
        }
    }
    Ok(files)
}

pub(super) fn parse_profile(name: &str) -> Option<u128> {
    let hex = name.strip_suffix(".inv")?;
    (hex.len() == 32 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| u128::from_str_radix(hex, 16).ok())
        .flatten()
        .filter(|&profile| profile != 0)
}

pub(super) fn parse_chunk(name: &str) -> Option<crate::world::ChunkKey> {
    let coordinates = name.strip_suffix(".bged")?;
    let mut parts = coordinates.split('_');
    let (x, y, z) = (parts.next()?, parts.next()?, parts.next()?);
    (parts.next().is_none()).then_some(crate::world::ChunkKey {
        x: x.parse().ok()?,
        y: y.parse().ok()?,
        z: z.parse().ok()?,
    })
}

fn allowed_root_name(name: &str) -> bool {
    matches!(
        name,
        "world.meta" | "content.map" | "drops.bin" | "server.wal" | "server.wal.manifest"
    ) || parse_chunk(name).is_some()
        || name
            .strip_prefix("server.wal.base.")
            .is_some_and(|suffix| suffix.parse::<u64>().is_ok())
        || name
            .strip_prefix("server.wal.tail.")
            .is_some_and(|suffix| suffix.parse::<u64>().is_ok())
}

fn fingerprint(path: &Path) -> io::Result<Fingerprint> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("legacy source contains a non-regular file"));
    }
    let mut checksum = 0xcbf29ce484222325u64;
    let mut length = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        length = length
            .checked_add(count as u64)
            .ok_or_else(|| invalid("legacy file length overflow"))?;
        for byte in &buffer[..count] {
            checksum = (checksum ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    }
    if length != metadata.len() {
        return Err(invalid("legacy file length changed during scan"));
    }
    Ok(Fingerprint {
        len: length,
        modified: metadata.modified().ok(),
        device: device(&metadata),
        inode: inode(&metadata),
        checksum,
    })
}

#[cfg(unix)]
fn device(meta: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.dev()
}

#[cfg(not(unix))]
fn device(_: &fs::Metadata) -> u64 {
    0
}

#[cfg(unix)]
fn inode(meta: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.ino()
}

#[cfg(not(unix))]
fn inode(_: &fs::Metadata) -> u64 {
    0
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

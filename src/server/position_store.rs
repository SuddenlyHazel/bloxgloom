//! Per-profile authoritative player positions. A missing file means a new player;
//! malformed files fail closed instead of silently replacing the last location.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAGIC: &[u8; 4] = b"BGPS";
const VERSION: u16 = 1;
const LEN: usize = 4 + 2 + 16 + 12 + 4;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct PositionStore {
    root: PathBuf,
}

impl PositionStore {
    pub(super) fn new(world_dir: &Path) -> io::Result<Self> {
        let root = world_dir.join("players");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path(&self, profile: u128) -> PathBuf {
        self.root.join(format!("{profile:032x}.pos"))
    }

    pub(super) fn load(&self, profile: u128) -> io::Result<Option<[f32; 3]>> {
        validate_profile(profile)?;
        let file = match File::open(self.path(profile)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take((LEN + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() != LEN
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != VERSION
            || u128::from_le_bytes(bytes[6..22].try_into().unwrap()) != profile
            || u32::from_le_bytes(bytes[34..38].try_into().unwrap()) != checksum(&bytes[..34])
        {
            return Err(invalid("invalid player position file"));
        }
        let position = std::array::from_fn(|index| {
            let start = 22 + index * 4;
            f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
        });
        validate_position(position)?;
        Ok(Some(position))
    }

    pub(super) fn save(&self, profile: u128, position: [f32; 3]) -> io::Result<()> {
        validate_profile(profile)?;
        validate_position(position)?;
        let mut bytes = Vec::with_capacity(LEN);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&profile.to_le_bytes());
        for coordinate in position {
            bytes.extend_from_slice(&coordinate.to_le_bytes());
        }
        bytes.extend_from_slice(&checksum(&bytes).to_le_bytes());
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
            fs::rename(&temporary, self.path(profile))?;
            File::open(&self.root)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}

fn validate_profile(profile: u128) -> io::Result<()> {
    if profile == 0 {
        Err(invalid("missing player profile"))
    } else {
        Ok(())
    }
}

fn validate_position(position: [f32; 3]) -> io::Result<()> {
    if position
        .iter()
        .all(|value| value.is_finite() && value.abs() < 1_000_000.0)
    {
        Ok(())
    } else {
        Err(invalid("invalid player position"))
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
mod tests;

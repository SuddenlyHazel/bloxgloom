//! Persistently reserved session-ID ranges keep durable launch/impact player
//! references distinct from sessions created after a server restart.
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const FILE: &str = "session-generation.bin";
const LOCK: &str = ".session-generation.lock";
const GENERATIONS: u32 = 1 << 31;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Reserve all 32-bit local session counters for one boot before any player is
/// announced. Crashes can waste a range; they cannot cause its reuse. A separate
/// stable lock inode serializes even two concurrent startup attempts.
pub(super) fn reserve(save: &Path) -> io::Result<u64> {
    fs::create_dir_all(save)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(save.join(LOCK))?;
    lock.lock()?;
    let generation = match fs::read(save.join(FILE)) {
        Ok(bytes) => {
            if bytes.len() != 12 || bytes[..4] != *b"BGS1" {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "invalid persisted player session generation",
                ));
            }
            let next = u32::from_le_bytes(bytes[4..8].try_into().expect("four bytes"));
            let complement = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes"));
            if next == 0 || complement != !next {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "corrupt player session generation",
                ));
            }
            next
        }
        Err(error) if error.kind() == ErrorKind::NotFound => 0,
        Err(error) => return Err(error),
    };
    if generation >= GENERATIONS {
        return Err(io::Error::new(
            ErrorKind::QuotaExceeded,
            "player session generation exhausted",
        ));
    }
    let temporary = save.join(format!(
        ".session-generation.{}.{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(b"BGS1")?;
        file.write_all(&(generation + 1).to_le_bytes())?;
        file.write_all(&(!(generation + 1)).to_le_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, save.join(FILE))?;
        File::open(save)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result?;
    Ok((u64::from(generation) << 32) | 1)
}

/// Counter zero is the exhausted sentinel at the next boot range's boundary.
/// The high bit belongs to the public transient-player entity namespace.
pub(super) fn next_after(id: u64) -> io::Result<u64> {
    if id & u64::from(u32::MAX) == 0 || id >= 1 << 63 {
        return Err(io::Error::new(
            ErrorKind::QuotaExceeded,
            "player session ID range exhausted",
        ));
    }
    id.checked_add(1)
        .ok_or_else(|| io::Error::new(ErrorKind::QuotaExceeded, "player session ID exhausted"))
}

#[cfg(test)]
mod tests;

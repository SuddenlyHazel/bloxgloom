//! Checkpoint elapsed phase separately from the last WAL command anchor.
use super::*;
use crate::server::journal::Journal;
use std::{collections::BTreeMap, fs, io::Write};

pub(in crate::server) struct Snapshot {
    pub anchor: Vec<u8>,
    pub elapsed_ms: u64,
}

pub(super) fn decode_anchor(bytes: &[u8]) -> io::Result<Option<(u64, u64)>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() != 16 {
        return Err(invalid());
    }
    let revision = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    let time = u64::from_le_bytes(bytes[8..].try_into().unwrap());
    if revision == 0 || time >= CYCLE_MS {
        return Err(invalid());
    }
    Ok(Some((revision, time)))
}

pub(super) fn read(root: &Path) -> io::Result<Snapshot> {
    let bytes = match fs::read(root.join("world.time")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Snapshot {
                anchor: Vec::new(),
                elapsed_ms: INITIAL_MS,
            });
        }
        Err(error) => return Err(error),
    };
    if bytes.len() != 28 || &bytes[..4] != b"BGT2" {
        return Err(invalid());
    }
    let anchor = if bytes[4..20] == [0; 16] {
        Vec::new()
    } else {
        bytes[4..20].to_vec()
    };
    decode_anchor(&anchor)?;
    let elapsed_ms = u64::from_le_bytes(bytes[20..].try_into().unwrap());
    if elapsed_ms >= CYCLE_MS {
        return Err(invalid());
    }
    Ok(Snapshot { anchor, elapsed_ms })
}

pub(super) fn save(path: &Path, snapshot: &Snapshot) -> io::Result<()> {
    decode_anchor(&snapshot.anchor)?;
    if snapshot.elapsed_ms >= CYCLE_MS {
        return Err(invalid());
    }
    let temporary = path.with_extension("time.tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(b"BGT2")?;
    file.write_all(if snapshot.anchor.is_empty() {
        &[0; 16]
    } else {
        &snapshot.anchor
    })?;
    file.write_all(&snapshot.elapsed_ms.to_le_bytes())?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    fs::File::open(path.parent().unwrap())?.sync_all()
}

/// Validate before any participant is replayed. A checkpoint matching the
/// latest command retains elapsed time; an older one resumes from that command.
/// The command anchor rides the common WAL base/tail, including rotation.
pub(in crate::server) fn prepare_recovery(
    root: &Path,
    journal: &Journal,
    latest: &BTreeMap<StateKey, Vec<u8>>,
) -> io::Result<Option<Snapshot>> {
    let current = read(root)?;
    for key in latest.keys().filter(|key| key.domain == DOMAIN) {
        if key != &state_key() {
            return Err(invalid());
        }
    }
    let Some(after) = latest.get(&state_key()) else {
        if !current.anchor.is_empty() {
            return Err(invalid());
        }
        return Ok(None);
    };
    let (_, elapsed_ms) = decode_anchor(after)?.ok_or_else(invalid)?;
    let (latest_revision, _) = decode_anchor(after)?.unwrap();
    let current_revision = decode_anchor(&current.anchor)?.map_or(0, |(revision, _)| revision);
    if current_revision >= latest_revision {
        // Equal revisions must be the exact command. A future checkpoint is
        // corruption. Older advisory phase samples may predate WAL rotation;
        // discard them and resume from the authoritative command instead.
        journal.validate_snapshot(&state_key(), &current.anchor)?;
        if current.anchor != *after {
            return Err(invalid());
        }
    }
    Ok((current.anchor != *after).then(|| Snapshot {
        anchor: after.clone(),
        elapsed_ms,
    }))
}

pub(in crate::server) fn replay(root: &Path, snapshot: &Snapshot) -> io::Result<()> {
    save(&root.join("world.time"), snapshot)
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid world time checkpoint or command",
    )
}

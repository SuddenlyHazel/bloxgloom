use super::*;
use crate::server::journal::Journal;
use std::{collections::BTreeMap, fs, io::Write};
pub(in crate::server) struct Checkpoint {
    pub anchor: Vec<u8>,
    pub snapshot: WeatherSnapshot,
}
pub(super) fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid weather checkpoint or command",
    )
}
pub(super) fn read(root: &Path, seed: u64) -> io::Result<Checkpoint> {
    let b = match fs::read(root.join("world.weather")) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Checkpoint {
                anchor: Vec::new(),
                snapshot: WeatherSnapshot::initial(seed),
            });
        }
        Err(e) => return Err(e),
    };
    if b.len() != 119 || &b[..4] != b"BGW1" {
        return Err(invalid());
    }
    let anchor = if b[4..61] == [0; 57] {
        Vec::new()
    } else {
        crate::weather::codec::decode(&b[4..61]).ok_or_else(invalid)?;
        b[4..61].to_vec()
    };
    let snapshot = crate::weather::codec::decode(&b[62..]).ok_or_else(invalid)?;
    if b[61] != 0 {
        return Err(invalid());
    }
    let anchor_revision = if anchor.is_empty() {
        0
    } else {
        crate::weather::codec::decode(&anchor)
            .ok_or_else(invalid)?
            .revision
    };
    if snapshot.revision != anchor_revision {
        return Err(invalid());
    }
    if !anchor.is_empty() {
        let command = crate::weather::codec::decode(&anchor).ok_or_else(invalid)?;
        if command.seed != snapshot.seed || command.elapsed_ms > snapshot.elapsed_ms {
            return Err(invalid());
        }
    }
    Ok(Checkpoint { anchor, snapshot })
}
pub(super) fn save(path: &Path, c: &Checkpoint) -> io::Result<()> {
    if !c.snapshot.valid()
        || (!c.anchor.is_empty() && crate::weather::codec::decode(&c.anchor).is_none())
    {
        return Err(invalid());
    }
    let temporary = path.with_extension("weather.tmp");
    let mut f = fs::File::create(&temporary)?;
    f.write_all(b"BGW1")?;
    f.write_all(if c.anchor.is_empty() {
        &[0; 57]
    } else {
        &c.anchor
    })?;
    f.write_all(&[0])?;
    f.write_all(&crate::weather::codec::encode(c.snapshot))?;
    f.sync_all()?;
    fs::rename(temporary, path)?;
    fs::File::open(path.parent().unwrap())?.sync_all()
}
pub(in crate::server) fn prepare_recovery(
    root: &Path,
    journal: &Journal,
    latest: &BTreeMap<StateKey, Vec<u8>>,
) -> io::Result<Option<Checkpoint>> {
    let current = read(root, 0)?;
    for key in latest.keys().filter(|k| k.domain == DOMAIN) {
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
    let snapshot = crate::weather::codec::decode(after).ok_or_else(invalid)?;
    let revision = if current.anchor.is_empty() {
        0
    } else {
        crate::weather::codec::decode(&current.anchor)
            .ok_or_else(invalid)?
            .revision
    };
    if revision >= snapshot.revision {
        journal.validate_snapshot(&state_key(), &current.anchor)?;
        if current.anchor != *after {
            return Err(invalid());
        }
    }
    Ok((current.anchor != *after).then(|| Checkpoint {
        anchor: after.clone(),
        snapshot,
    }))
}
pub(in crate::server) fn replay(root: &Path, c: &Checkpoint) -> io::Result<()> {
    save(&root.join("world.weather"), c)
}

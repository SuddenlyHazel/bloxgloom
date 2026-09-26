//! Off-tick per-key checkpoints for WAL-durable fire state.
//!
//! A file path identifies exactly one registered fire key, while the file
//! envelope repeats that key and checksums the payload. Crash-left temporary
//! files are identified strictly and can be removed after full validation.

use super::codec::{checksum, invalid};
use super::scheduler::FireRecovered;
use crate::server::journal::StateKey;
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[path = "checkpoint/aggregate.rs"]
mod aggregate;

const MAX_FIRE_VALUE_BYTES: usize = 128 * 1024;
const MAX_FIRE_FILE_BYTES: usize = MAX_FIRE_VALUE_BYTES + 96;

#[derive(Clone)]
pub(in crate::server) struct FireCheckpointStore {
    root: PathBuf,
    values: Arc<Mutex<Option<BTreeMap<StateKey, Vec<u8>>>>>,
}

impl FireCheckpointStore {
    pub(in crate::server) fn new(world_dir: &Path) -> io::Result<Self> {
        let root = world_dir.join("fire");
        fs::create_dir_all(&root)?;
        let values = aggregate::read_snapshot_file(&root.join(aggregate::FILE_NAME))?;
        Ok(Self {
            root,
            values: Arc::new(Mutex::new(values)),
        })
    }

    pub(in crate::server) fn read(&self, key: &StateKey) -> io::Result<Option<Vec<u8>>> {
        if let Some(values) = self
            .values
            .lock()
            .map_err(|_| io::Error::other("fire checkpoint state poisoned"))?
            .as_ref()
        {
            return Ok(values.get(key).cloned());
        }
        self.read_legacy(key)
    }

    fn read_legacy(&self, key: &StateKey) -> io::Result<Option<Vec<u8>>> {
        let path = self.path(key)?;
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take((MAX_FIRE_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FIRE_FILE_BYTES {
            return Err(invalid("fire checkpoint file too large"));
        }
        let value = decode_envelope(key, &bytes)?;
        Ok(Some(value))
    }

    /// The generic checkpoint worker calls this only after a WAL receipt.
    /// Empty after-values unlink the exact old key rather than leaving a
    /// tombstone that might be mistaken for live work after rotation. The
    /// physical checkpoint is a complete-map aggregate, replaced atomically.
    #[cfg(test)]
    pub(in crate::server) fn write(&self, key: &StateKey, value: &[u8]) -> io::Result<()> {
        self.write_batch(&[(key.clone(), value.to_vec())])
    }

    /// Persist several post-WAL fire snapshots with one file and directory
    /// fence. The in-memory map is owned by the checkpoint worker and is only
    /// changed after the new complete snapshot has been atomically installed.
    pub(in crate::server) fn write_batch(&self, updates: &[(StateKey, Vec<u8>)]) -> io::Result<()> {
        if updates.is_empty() {
            return Ok(());
        }
        let mut seen = HashSet::with_capacity(updates.len());
        for (key, value) in updates {
            validate_value(key, value)?;
            if !seen.insert(key) {
                return Err(invalid("duplicate key in fire checkpoint batch"));
            }
        }

        let mut state = self
            .values
            .lock()
            .map_err(|_| io::Error::other("fire checkpoint state poisoned"))?;
        let mut next = match state.as_ref() {
            Some(values) => values.clone(),
            None => self.load_legacy_values()?,
        };
        for (key, value) in updates {
            if value.is_empty() {
                next.remove(key);
            } else {
                next.insert(key.clone(), value.clone());
            }
        }
        aggregate::write_snapshot_file(&self.root.join(aggregate::FILE_NAME), &next)?;
        *state = Some(next);
        Ok(())
    }

    /// Run in startup's read-only validation pass, before any replay write.
    /// A complete live file without a WAL frontier is an orphan and rejected;
    /// recognized interrupted temporary files are harmless and not treated
    /// as current state, but unknown filenames fail closed.
    pub(in crate::server) fn validate_no_orphans(
        &self,
        latest: &BTreeMap<StateKey, Vec<u8>>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(invalid("unexpected non-file in fire checkpoint directory"));
            }
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| invalid("non-UTF8 fire checkpoint filename"))?;
            if is_interrupted_temporary(name) {
                continue;
            }
            if name == aggregate::FILE_NAME {
                let snapshot = aggregate::read_snapshot_file(&entry.path())?
                    .ok_or_else(|| invalid("fire checkpoint aggregate disappeared"))?;
                if snapshot.keys().any(|key| !latest.contains_key(key)) {
                    return Err(invalid("fire checkpoint aggregate has no journal frontier"));
                }
                continue;
            }
            let key = parse_filename(name)?;
            if !latest.contains_key(&key) {
                return Err(invalid("fire checkpoint has no journal frontier"));
            }
            // Validate old per-key files even when the aggregate supersedes
            // them. They remain save data until an explicit format migration.
            self.read_legacy(&key)?;
        }
        Ok(())
    }

    /// Run only after all candidate/current snapshots have been validated.
    pub(in crate::server) fn cleanup_interrupted_temps(&self) -> io::Result<()> {
        let mut removed = false;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| invalid("non-UTF8 fire checkpoint filename"))?;
            if is_interrupted_temporary(name) {
                fs::remove_file(entry.path())?;
                removed = true;
            }
        }
        if removed {
            File::open(&self.root)?.sync_all()?;
        }
        Ok(())
    }

    fn path(&self, key: &StateKey) -> io::Result<PathBuf> {
        Ok(self.root.join(filename(key)?))
    }

    fn load_legacy_values(&self) -> io::Result<BTreeMap<StateKey, Vec<u8>>> {
        let mut values = BTreeMap::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(invalid("unexpected non-file in fire checkpoint directory"));
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("non-UTF8 fire checkpoint filename"))?;
            if is_interrupted_temporary(&name) || name == aggregate::FILE_NAME {
                continue;
            }
            let key = parse_filename(&name)?;
            let value = self
                .read_legacy(&key)?
                .ok_or_else(|| invalid("fire checkpoint disappeared while batching"))?;
            if !value.is_empty() {
                values.insert(key, value);
            }
        }
        Ok(values)
    }
}

fn is_interrupted_temporary(name: &str) -> bool {
    parse_temporary(name).is_some() || aggregate::is_snapshot_temporary(name)
}

#[cfg(test)]
pub(super) fn encode_envelope(key: &StateKey, value: &[u8]) -> io::Result<Vec<u8>> {
    validate_value(key, value)?;
    let domain = domain_tag(key)?;
    let mut bytes = Vec::with_capacity(14 + key.bytes.len() + value.len());
    bytes.extend(b"BGFV");
    bytes.push(1);
    bytes.push(domain);
    bytes.push(key.bytes.len() as u8);
    bytes.extend(&key.bytes);
    bytes.extend((value.len() as u32).to_le_bytes());
    bytes.extend(value);
    bytes.extend(checksum(&bytes).to_le_bytes());
    Ok(bytes)
}

fn decode_envelope(key: &StateKey, bytes: &[u8]) -> io::Result<Vec<u8>> {
    let domain = domain_tag(key)?;
    if bytes.len() < 15 || &bytes[..4] != b"BGFV" || bytes[4] != 1 || bytes[5] != domain {
        return Err(invalid("invalid fire checkpoint envelope"));
    }
    let key_len = usize::from(bytes[6]);
    if key_len != key.bytes.len() || bytes.len() < 15 + key_len {
        return Err(invalid("fire checkpoint key length mismatch"));
    }
    if bytes[7..7 + key_len] != key.bytes {
        return Err(invalid("fire checkpoint key mismatch"));
    }
    let length_at = 7 + key_len;
    let value_len =
        u32::from_le_bytes(bytes[length_at..length_at + 4].try_into().unwrap()) as usize;
    let expected = 15usize
        .checked_add(key_len)
        .and_then(|size| size.checked_add(value_len))
        .ok_or_else(|| invalid("fire checkpoint envelope too large"))?;
    if bytes.len() != expected || value_len > MAX_FIRE_VALUE_BYTES {
        return Err(invalid("fire checkpoint value length mismatch"));
    }
    let checksum_at = expected - 4;
    if checksum(&bytes[..checksum_at])
        != u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
    {
        return Err(invalid("fire checkpoint checksum mismatch"));
    }
    let value = bytes[length_at + 4..checksum_at].to_vec();
    validate_value(key, &value)?;
    Ok(value)
}

fn validate_value(key: &StateKey, value: &[u8]) -> io::Result<()> {
    if value.len() > MAX_FIRE_VALUE_BYTES {
        return Err(invalid("fire checkpoint value too large"));
    }
    if !FireRecovered::default().apply_value(key, value)? {
        return Err(invalid("unknown fire checkpoint domain"));
    }
    Ok(())
}

fn domain_tag(key: &StateKey) -> io::Result<u8> {
    match (key.domain.as_str(), key.bytes.len()) {
        ("bloxgloom:fire_frontier", 12) => Ok(1),
        ("bloxgloom:fire_pending", 24) => Ok(2),
        ("bloxgloom:fire_cursor", 1) if key.bytes[0] < 64 => Ok(3),
        _ => Err(invalid("invalid fire checkpoint key")),
    }
}

fn filename(key: &StateKey) -> io::Result<String> {
    let prefix = match domain_tag(key)? {
        1 => "frontier",
        2 => "pending",
        3 => "cursor",
        _ => unreachable!(),
    };
    let mut name = String::with_capacity(prefix.len() + key.bytes.len() * 2 + 6);
    name.push_str(prefix);
    name.push('_');
    for byte in &key.bytes {
        name.push(char::from_digit(u32::from(byte >> 4), 16).unwrap());
        name.push(char::from_digit(u32::from(byte & 15), 16).unwrap());
    }
    name.push_str(".fire");
    Ok(name)
}

fn parse_filename(name: &str) -> io::Result<StateKey> {
    let (domain, hex) = if let Some(hex) = name
        .strip_prefix("frontier_")
        .and_then(|tail| tail.strip_suffix(".fire"))
    {
        ("bloxgloom:fire_frontier", hex)
    } else if let Some(hex) = name
        .strip_prefix("pending_")
        .and_then(|tail| tail.strip_suffix(".fire"))
    {
        ("bloxgloom:fire_pending", hex)
    } else if let Some(hex) = name
        .strip_prefix("cursor_")
        .and_then(|tail| tail.strip_suffix(".fire"))
    {
        ("bloxgloom:fire_cursor", hex)
    } else {
        return Err(invalid("unknown fire checkpoint filename"));
    };
    if hex.len() % 2 != 0
        || hex
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
    {
        return Err(invalid("invalid fire checkpoint filename"));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks_exact(2) {
        let text =
            std::str::from_utf8(pair).map_err(|_| invalid("invalid fire checkpoint filename"))?;
        bytes.push(
            u8::from_str_radix(text, 16)
                .map_err(|_| invalid("invalid fire checkpoint filename"))?,
        );
    }
    let key = StateKey::new(domain, bytes);
    domain_tag(&key)?;
    if filename(&key)? != name {
        return Err(invalid("non-canonical fire checkpoint filename"));
    }
    Ok(key)
}

fn parse_temporary(name: &str) -> Option<StateKey> {
    let body = name.strip_prefix('.')?.strip_suffix(".tmp")?;
    let (base_and_pid, sequence) = body.rsplit_once('.')?;
    let (base, pid) = base_and_pid.rsplit_once('.')?;
    if pid.parse::<u32>().ok()? == 0 || sequence.parse::<u64>().ok()? == 0 {
        return None;
    }
    parse_filename(base).ok()
}

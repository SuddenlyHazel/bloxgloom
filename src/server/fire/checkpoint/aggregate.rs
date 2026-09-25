//! Atomic encoding and replacement of the complete fire checkpoint map.

use super::{domain_tag, invalid, validate_value};
use crate::server::journal::StateKey;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) const FILE_NAME: &str = "checkpoints.fire";
const MAGIC: &[u8; 4] = b"BGFC";
const VERSION: u8 = 1;
const MAX_SNAPSHOT_BYTES: usize = 512 * 1024 * 1024;
const MAX_SNAPSHOT_KEYS: usize = 1_000_000;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn read_snapshot_file(path: &Path) -> io::Result<Option<BTreeMap<StateKey, Vec<u8>>>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take((MAX_SNAPSHOT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(invalid("fire checkpoint aggregate too large"));
    }
    decode_snapshot(&bytes).map(Some)
}

pub(super) fn write_snapshot_file(
    path: &Path,
    values: &BTreeMap<StateKey, Vec<u8>>,
) -> io::Result<()> {
    let bytes = encode_snapshot(values)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("invalid fire checkpoint aggregate path"))?;
    let temporary = parent.join(format!(
        ".{FILE_NAME}.{}.{}.tmp",
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
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn is_snapshot_temporary(name: &str) -> bool {
    let Some(body) = name
        .strip_prefix(".checkpoints.fire.")
        .and_then(|body| body.strip_suffix(".tmp"))
    else {
        return false;
    };
    let Some((pid, sequence)) = body.split_once('.') else {
        return false;
    };
    pid.parse::<u32>().is_ok_and(|value| value != 0)
        && sequence.parse::<u64>().is_ok_and(|value| value != 0)
}

fn encode_snapshot(values: &BTreeMap<StateKey, Vec<u8>>) -> io::Result<Vec<u8>> {
    if values.len() > MAX_SNAPSHOT_KEYS {
        return Err(invalid("too many fire checkpoint aggregate keys"));
    }
    let mut bytes = Vec::with_capacity(9 + values.len() * 40);
    bytes.extend(MAGIC);
    bytes.push(VERSION);
    bytes.extend(
        u32::try_from(values.len())
            .map_err(|_| invalid("too many fire checkpoint aggregate keys"))?
            .to_le_bytes(),
    );
    for (key, value) in values {
        if value.is_empty() {
            return Err(invalid("empty fire checkpoint aggregate value"));
        }
        validate_value(key, value)?;
        bytes.push(domain_tag(key)?);
        bytes.push(
            u8::try_from(key.bytes.len()).map_err(|_| invalid("invalid fire checkpoint key"))?,
        );
        bytes.extend(&key.bytes);
        bytes.extend(
            u32::try_from(value.len())
                .map_err(|_| invalid("fire checkpoint value too large"))?
                .to_le_bytes(),
        );
        bytes.extend(value);
        if bytes.len().saturating_add(4) > MAX_SNAPSHOT_BYTES {
            return Err(invalid("fire checkpoint aggregate too large"));
        }
    }
    bytes.extend(super::checksum(&bytes).to_le_bytes());
    Ok(bytes)
}

fn decode_snapshot(bytes: &[u8]) -> io::Result<BTreeMap<StateKey, Vec<u8>>> {
    if bytes.len() < 13 || &bytes[..4] != MAGIC || bytes[4] != VERSION {
        return Err(invalid("invalid fire checkpoint aggregate"));
    }
    if super::checksum(&bytes[..bytes.len() - 4])
        != u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap())
    {
        return Err(invalid("fire checkpoint aggregate checksum mismatch"));
    }

    let count = u32::from_le_bytes(bytes[5..9].try_into().unwrap()) as usize;
    if count > MAX_SNAPSHOT_KEYS {
        return Err(invalid("too many fire checkpoint aggregate keys"));
    }
    let checksum_at = bytes.len() - 4;
    let mut cursor = 9usize;
    let mut values = BTreeMap::new();
    let mut previous: Option<StateKey> = None;
    for _ in 0..count {
        if cursor.checked_add(2).is_none_or(|end| end > checksum_at) {
            return Err(invalid("truncated fire checkpoint aggregate"));
        }
        let domain = match bytes[cursor] {
            1 => "bloxgloom:fire_frontier",
            2 => "bloxgloom:fire_pending",
            3 => "bloxgloom:fire_cursor",
            _ => return Err(invalid("invalid fire checkpoint aggregate domain")),
        };
        let key_len = usize::from(bytes[cursor + 1]);
        cursor += 2;
        let key_end = cursor
            .checked_add(key_len)
            .ok_or_else(|| invalid("fire checkpoint aggregate too large"))?;
        let length_end = key_end
            .checked_add(4)
            .ok_or_else(|| invalid("fire checkpoint aggregate too large"))?;
        if length_end > checksum_at {
            return Err(invalid("truncated fire checkpoint aggregate"));
        }
        let key = StateKey::new(domain, bytes[cursor..key_end].to_vec());
        domain_tag(&key)?;
        if previous.as_ref().is_some_and(|previous| previous >= &key) {
            return Err(invalid("non-canonical fire checkpoint aggregate key order"));
        }
        let value_len = u32::from_le_bytes(bytes[key_end..length_end].try_into().unwrap()) as usize;
        if value_len == 0 {
            return Err(invalid("empty fire checkpoint aggregate value"));
        }
        let value_end = length_end
            .checked_add(value_len)
            .ok_or_else(|| invalid("fire checkpoint aggregate too large"))?;
        if value_len > super::MAX_FIRE_VALUE_BYTES || value_end > checksum_at {
            return Err(invalid("invalid fire checkpoint aggregate value length"));
        }
        let value = bytes[length_end..value_end].to_vec();
        validate_value(&key, &value)?;
        previous = Some(key.clone());
        values.insert(key, value);
        cursor = value_end;
    }
    if cursor != checksum_at {
        return Err(invalid("fire checkpoint aggregate length mismatch"));
    }
    Ok(values)
}

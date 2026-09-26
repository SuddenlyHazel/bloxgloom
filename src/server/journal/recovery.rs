//! Startup journal opening, tail recovery, and snapshot-chain validation.

use super::codec::{decode_transaction, frame_checksum, invalid_data};
use super::rotation;
use super::{
    FILE_HEADER_LEN, FILE_MAGIC, FILE_VERSION, FRAME_OVERHEAD, Journal, KnownRecord,
    MAX_JOURNAL_BYTES, MAX_RECORD_BYTES, StateKey, Transaction,
};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

impl Journal {
    /// Opens or creates a journal and recovers an incomplete trailing frame.
    /// Complete frames with a bad checksum or invalid contents are rejected.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_owned();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        let manifest_path = rotation::manifest_path(&path)?;
        let manifest = if manifest_path.exists() {
            Some(rotation::read_manifest(&path)?)
        } else {
            None
        };

        let (
            mut file,
            initial_latest,
            base_anchor,
            generation,
            drop_owner_set_closed,
            mut physical_records,
            next_id_watermark,
        ) = if let Some(manifest) = &manifest {
            let base = rotation::read_base(&path, manifest)?;
            let (file, _) = rotation::open_tail(&path, manifest)?;
            if base.generation != manifest.generation
                || base.cut_sequence != manifest.cut_sequence
                || base.next_transaction_id != manifest.next_transaction_id
            {
                return Err(invalid_data(
                    "journal generation base does not match its manifest",
                ));
            }
            (
                file,
                base.values.clone(),
                base.values,
                manifest.generation,
                base.drop_owner_set_closed,
                manifest.cut_sequence,
                manifest.next_transaction_id,
            )
        } else {
            let file = open_or_create_legacy(&path)?;
            let length = file.metadata()?.len();
            if length > MAX_JOURNAL_BYTES {
                return Err(invalid_data("journal exceeds its configured size limit"));
            }
            (file, HashMap::new(), HashMap::new(), 0, false, 0, 1)
        };
        let mut next_transaction_id = next_id_watermark;
        let mut latest: std::collections::BTreeMap<_, _> = initial_latest.into_iter().collect();
        let mut base_anchor = base_anchor;
        let mut max_tick = match latest.remove(&super::clock_key()) {
            Some(bytes) => u64::from_le_bytes(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| invalid_data("invalid journal clock metadata"))?,
            ),
            None => 0,
        };
        base_anchor.remove(&super::clock_key());
        if latest.keys().any(|key| key.domain == super::CLOCK_DOMAIN) {
            return Err(invalid_data("invalid journal clock key"));
        }

        let length = file.metadata()?.len();
        if length > MAX_JOURNAL_BYTES {
            return Err(invalid_data("journal exceeds its configured size limit"));
        }
        let header_len = if manifest.is_some() {
            rotation::tail_header_len()
        } else {
            FILE_HEADER_LEN
        };
        if length < header_len as u64 {
            return Err(invalid_data("truncated journal header"));
        }
        if manifest.is_none() {
            validate_legacy_header(&mut file)?;
        }

        let mut records = Vec::<Transaction>::new();
        let mut known = HashMap::<u128, KnownRecord>::new();
        let mut history = HashMap::<StateKey, Vec<(usize, usize)>>::new();
        let mut offset = header_len as u64;
        while offset < length {
            let remaining = length - offset;
            if remaining < 4 {
                truncate_tail(&mut file, offset)?;
                break;
            }

            file.seek(SeekFrom::Start(offset))?;
            let mut length_bytes = [0u8; 4];
            file.read_exact(&mut length_bytes)?;
            let payload_len = u32::from_le_bytes(length_bytes) as usize;
            if payload_len == 0 || payload_len > MAX_RECORD_BYTES {
                return Err(invalid_data("journal record exceeds size limit"));
            }
            let frame_len = FRAME_OVERHEAD
                .checked_add(payload_len)
                .ok_or_else(|| invalid_data("journal record length overflow"))?;
            if remaining < frame_len as u64 {
                truncate_tail(&mut file, offset)?;
                break;
            }

            let mut payload = vec![0; payload_len];
            file.read_exact(&mut payload)?;
            let mut checksum_bytes = [0u8; 4];
            file.read_exact(&mut checksum_bytes)?;
            let expected = u32::from_le_bytes(checksum_bytes);
            let actual = frame_checksum(&length_bytes, &payload);
            if expected != actual {
                return Err(invalid_data("journal committed-record checksum mismatch"));
            }

            let transaction = decode_transaction(&payload)?;
            max_tick = max_tick.max(transaction.tick);
            physical_records = physical_records
                .checked_add(1)
                .ok_or_else(|| invalid_data("journal sequence exhausted"))?;
            if let Some(previous) = known.get(&transaction.id) {
                if records[previous.index] != transaction {
                    return Err(invalid_data(
                        "journal transaction ID reused with different data",
                    ));
                }
                // Exact duplicate IDs are safe to encounter after an uncertain
                // retry, but only the first copy participates in replay.
            } else {
                if manifest.is_some() && transaction.id < next_transaction_id {
                    return Err(invalid_data(
                        "journal tail transaction ID is below its generation watermark",
                    ));
                }
                for change in &transaction.changes {
                    if latest
                        .get(&change.key)
                        .is_some_and(|after| after != &change.before)
                    {
                        return Err(invalid_data(
                            "journal state-key before value breaks committed history",
                        ));
                    }
                }
                for change in &transaction.changes {
                    latest.insert(change.key.clone(), change.after.clone());
                }
                let index = records.len();
                for (change_index, change) in transaction.changes.iter().enumerate() {
                    history
                        .entry(change.key.clone())
                        .or_default()
                        .push((index, change_index));
                }
                known.insert(
                    transaction.id,
                    KnownRecord {
                        index,
                        sequence: physical_records,
                    },
                );
                records.push(transaction);
                next_transaction_id = next_transaction_id.max(
                    records
                        .last()
                        .expect("just pushed record")
                        .id
                        .checked_add(1)
                        .ok_or_else(|| invalid_data("journal transaction ID space exhausted"))?,
                );
            }
            offset += frame_len as u64;
        }

        // A partial trailing frame may have been truncated above. Base the
        // live byte counter on the recovered file, not the pre-recovery size.
        let recovered_length = file.metadata()?.len();
        file.seek(SeekFrom::End(0))?;
        Ok(Self {
            path,
            file,
            manifest,
            generation,
            base_anchor,
            drop_owner_set_closed,
            next_transaction_id,
            max_tick,
            records,
            known,
            latest,
            history,
            physical_records,
            log_bytes: recovered_length,
            poisoned: None,
        })
    }

    /// Ensures an existing atomic snapshot is a reachable point in this
    /// key's WAL history: the initial before-value or any committed after-value.
    /// This permits checkpoint lag/partial multi-file checkpoints, but rejects
    /// unrelated valid files rather than overwriting them during recovery.
    pub fn validate_snapshot(&self, key: &StateKey, current: &[u8]) -> io::Result<()> {
        let base_matches = self
            .base_anchor
            .get(key)
            .is_some_and(|value| value == current);
        let Some(history) = self.history.get(key) else {
            if base_matches || !self.base_anchor.contains_key(key) {
                return Ok(());
            }
            return Err(invalid_data("snapshot does not match its journal history"));
        };
        let first = history.first().expect("history has at least one change");
        let first_before = &self.records[first.0].changes[first.1].before;
        let matched = history
            .iter()
            .any(|&(record, change)| self.records[record].changes[change].after == current);
        if base_matches
            || (!self.base_anchor.contains_key(key) && first_before == current)
            || matched
        {
            Ok(())
        } else {
            Err(invalid_data("snapshot does not match its journal history"))
        }
    }
}

pub(super) fn truncate_tail(file: &mut File, offset: u64) -> io::Result<()> {
    file.set_len(offset)?;
    file.sync_all()
}

fn open_or_create_legacy(path: &Path) -> io::Result<File> {
    let mut file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            write_legacy_header(&mut file)?;
            sync_parent(path)?;
            file
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            OpenOptions::new().read(true).write(true).open(path)?
        }
        Err(error) => return Err(error),
    };

    let length = file.metadata()?.len();
    if length < FILE_HEADER_LEN as u64 {
        let mut existing = vec![0; length as usize];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut existing)?;
        let expected = legacy_header();
        if existing.as_slice() != &expected[..existing.len()] {
            return Err(invalid_data(
                "truncated journal header is not a valid header prefix",
            ));
        }
        // Rewriting from byte zero keeps every crash point an exact prefix of
        // the known header. A nonprefix is never guessed or repaired.
        file.set_len(0)?;
        file.sync_all()?;
        file.seek(SeekFrom::Start(0))?;
        write_legacy_header(&mut file)?;
        sync_parent(path)?;
    }
    Ok(file)
}

fn validate_legacy_header(file: &mut File) -> io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; FILE_HEADER_LEN];
    file.read_exact(&mut header)?;
    if &header[..4] != FILE_MAGIC
        || u16::from_le_bytes(header[4..6].try_into().expect("header version")) != FILE_VERSION
        || u32::from_le_bytes(header[6..10].try_into().expect("header checksum"))
            != super::codec::crc32(&header[..6])
    {
        return Err(invalid_data("invalid or unsupported journal header"));
    }
    Ok(())
}

pub(super) fn legacy_header() -> [u8; FILE_HEADER_LEN] {
    let mut header = [0u8; FILE_HEADER_LEN];
    header[..4].copy_from_slice(FILE_MAGIC);
    header[4..6].copy_from_slice(&FILE_VERSION.to_le_bytes());
    let checksum = super::codec::crc32(&header[..6]);
    header[6..].copy_from_slice(&checksum.to_le_bytes());
    header
}

fn write_legacy_header(file: &mut File) -> io::Result<()> {
    file.write_all(&legacy_header())?;
    file.sync_all()
}

pub(super) fn sync_parent(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)?.sync_all()
}

//! Versioned write-ahead journal for durable server transactions.
//!
//! A transaction contains complete, opaque before/after values for every key it
//! changes. This deliberately leaves BGED/BGIN/BGDP decoding to their existing
//! stores: the journal records transitions without reinterpreting save data.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const FILE_MAGIC: &[u8; 4] = b"BGWJ";
const FILE_VERSION: u16 = 1;
const FILE_HEADER_LEN: usize = 4 + 2 + 4;
const RECORD_VERSION: u16 = 1;
const FRAME_OVERHEAD: usize = 4 + 4;
const MAX_RECORD_BYTES: usize = 1024 * 1024;
const MAX_DOMAIN_BYTES: usize = 64;
const MAX_KEY_BYTES: usize = 4096;
const MAX_CHANGES: usize = 65_535;
const MAX_QUEUE_CAPACITY: usize = 256;
const MAX_BATCH_RECORDS: usize = 128;
/// Hard fail-closed bound for one append-only WAL tail.
pub const MAX_JOURNAL_BYTES: u64 = 256 * 1024 * 1024;
/// Advisory threshold at which the server should start its checkpoint-gated
/// rotation flow. The hard cap remains authoritative if callers do not rotate.
pub const JOURNAL_ROTATION_SOFT_LIMIT_BYTES: u64 = 224 * 1024 * 1024;

mod rotation;

/// Maximum encoded transaction payload, excluding its length and checksum.
pub const MAX_TRANSACTION_BYTES: usize = MAX_RECORD_BYTES;

/// Stable identity of one server-owned value. The domain is a short namespaced
/// kind such as `chunk_override`, `inventory`, or `drop`; `bytes` is its full
/// key (for example, chunk coordinates or a profile ID).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateKey {
    pub domain: String,
    pub bytes: Vec<u8>,
}

impl StateKey {
    pub fn new(domain: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            domain: domain.into(),
            bytes: bytes.into(),
        }
    }
}

/// A single key transition. Empty byte vectors can represent a missing value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub key: StateKey,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}

impl Change {
    pub fn new(key: StateKey, before: impl Into<Vec<u8>>, after: impl Into<Vec<u8>>) -> Self {
        Self {
            key,
            before: before.into(),
            after: after.into(),
        }
    }
}

/// Atomic set of full key transitions created by one durable gameplay action.
/// ID zero is reserved; callers should derive nonzero IDs from stable command
/// identities, for example `(connection ID, client sequence)` or a producer
/// sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub id: u128,
    pub tick: u64,
    pub changes: Vec<Change>,
}

impl Transaction {
    pub fn new(id: u128, tick: u64, changes: Vec<Change>) -> Self {
        Self { id, tick, changes }
    }

    /// Idempotently reapplies this transaction's exact `after` values. The read
    /// callback returns an empty vector for a missing key; callers should use a
    /// distinct encoding if an actual stored value can itself be empty. All
    /// keys are checked before the first write, and a partially completed set
    /// can safely be retried after a crash.
    pub fn redo<T>(
        &self,
        target: &mut T,
        mut read: impl FnMut(&mut T, &StateKey) -> io::Result<Vec<u8>>,
        mut write: impl FnMut(&mut T, &StateKey, &[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        let transaction = self.clone().canonicalize()?;
        let mut pending = Vec::with_capacity(transaction.changes.len());
        for change in &transaction.changes {
            let current = read(target, &change.key)?;
            if current == change.after {
                continue;
            }
            if current != change.before {
                return Err(invalid_data_owned(format!(
                    "journal transaction {} replay precondition mismatch for {} key",
                    transaction.id, change.key.domain
                )));
            }
            pending.push(change);
        }
        for change in pending {
            write(target, &change.key, &change.after)?;
        }
        Ok(())
    }

    fn canonicalize(mut self) -> io::Result<Self> {
        self.changes.sort_by(|left, right| left.key.cmp(&right.key));
        validate_transaction(&self, true)?;
        Ok(self)
    }
}

/// An entry reported after its complete frame has been synced to disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitReceipt {
    pub id: u128,
    /// One-based global physical record position across all generations.
    pub sequence: u64,
    /// True when this request repeated an already durable identical ID.
    pub duplicate: bool,
}

/// Nonblocking submission failure. `Full` is backpressure; callers should keep
/// the staged action pending or reject it, never apply it as committed.
#[derive(Debug)]
pub enum SubmitError {
    Full,
    Closed,
    Invalid(io::Error),
}

impl std::fmt::Display for SubmitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => formatter.write_str("journal request queue is full"),
            Self::Closed => formatter.write_str("journal writer is closed"),
            Self::Invalid(error) => write!(formatter, "invalid journal transaction: {error}"),
        }
    }
}

impl std::error::Error for SubmitError {}

struct KnownRecord {
    index: usize,
    sequence: u64,
}

/// Open, recovered legacy journal or base-plus-tail generation. Startup replay
/// is synchronous by design; move it into a [`JournalWriter`] before serving.
pub struct Journal {
    path: std::path::PathBuf,
    file: File,
    manifest: Option<rotation::Manifest>,
    generation: u64,
    base_anchor: HashMap<StateKey, Vec<u8>>,
    next_transaction_id: u128,
    records: Vec<Transaction>,
    known: HashMap<u128, KnownRecord>,
    latest: HashMap<StateKey, Vec<u8>>,
    history: HashMap<StateKey, Vec<(usize, usize)>>,
    physical_records: u64,
    log_bytes: u64,
    poisoned: Option<(io::ErrorKind, String)>,
}

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
                manifest.cut_sequence,
                manifest.next_transaction_id,
            )
        } else {
            let file = open_or_create_legacy(&path)?;
            let length = file.metadata()?.len();
            if length > MAX_JOURNAL_BYTES {
                return Err(invalid_data("journal exceeds its configured size limit"));
            }
            (file, HashMap::new(), HashMap::new(), 0, 0, 1)
        };
        let mut next_transaction_id = next_id_watermark;
        let mut latest = initial_latest;

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
            next_transaction_id,
            records,
            known,
            latest,
            history,
            physical_records,
            log_bytes: recovered_length,
            poisoned: None,
        })
    }

    /// Unique transactions in current-tail log order. A rotated base has
    /// already materialized all state at its cut, so only its tail is returned.
    /// Exact duplicate IDs are represented once; conflicts fail `open`.
    pub fn records(&self) -> &[Transaction] {
        &self.records
    }

    /// Next nonzero monotonically increasing ID for a server-owned action.
    /// The base persists this watermark so rotation never makes old IDs reusable.
    pub fn next_id(&self) -> io::Result<u128> {
        (self.next_transaction_id != 0)
            .then_some(self.next_transaction_id)
            .ok_or_else(|| invalid_data("journal transaction ID space exhausted"))
    }

    /// Latest globally synced physical WAL sequence, preserved across rotations.
    pub fn sequence(&self) -> u64 {
        self.physical_records
    }

    /// Bytes in the active legacy WAL or append-only generation tail.
    pub fn bytes(&self) -> u64 {
        self.log_bytes
    }

    /// Final committed value for every base or tail key. Since saves are
    /// independent atomic files, startup applies this complete final set before
    /// accepting clients; repeating the writes is idempotent.
    pub fn latest_values(&self) -> BTreeMap<StateKey, Vec<u8>> {
        self.latest
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
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
            || (self.base_anchor.get(key).is_none() && first_before == current)
            || matched
        {
            Ok(())
        } else {
            Err(invalid_data("snapshot does not match its journal history"))
        }
    }

    /// Replays each unique transaction in the current tail in log order. The
    /// base state was materialized at its cut, so compacted history is omitted.
    /// The callback should redo `after` values idempotently.
    pub fn replay(&self, mut apply: impl FnMut(&Transaction) -> io::Result<()>) -> io::Result<()> {
        for record in &self.records {
            apply(record)?;
        }
        Ok(())
    }

    /// Starts a bounded writer thread. Submissions never wait for disk I/O;
    /// acknowledgments arrive only after `sync_all` succeeds.
    pub fn into_writer(
        self,
        queue_capacity: usize,
        batch_delay: Duration,
    ) -> io::Result<JournalWriter> {
        let (sender, receiver) = mpsc::sync_channel(queue_capacity.clamp(1, MAX_QUEUE_CAPACITY));
        let usage = Arc::new(AtomicU64::new(self.log_bytes));
        let sequence = Arc::new(AtomicU64::new(self.physical_records));
        let worker_usage = Arc::clone(&usage);
        let worker_sequence = Arc::clone(&sequence);
        let worker = thread::Builder::new()
            .name("bloxgloom-journal".into())
            .spawn(move || {
                writer_loop(self, receiver, batch_delay, worker_usage, worker_sequence)
            })?;
        Ok(JournalWriter {
            sender: Some(sender),
            worker: Some(worker),
            usage,
            sequence,
        })
    }

    fn append_batch(&mut self, requests: Vec<Request>) -> Vec<io::Result<CommitReceipt>> {
        if let Some((kind, message)) = &self.poisoned {
            return requests
                .iter()
                .map(|_| Err(io::Error::new(*kind, message.clone())))
                .collect();
        }

        enum Status {
            Existing(CommitReceipt),
            Candidate(usize),
            Repeated(usize),
            Error(io::ErrorKind, String),
        }

        let mut statuses = Vec::with_capacity(requests.len());
        let mut candidates = Vec::<Transaction>::new();
        let mut in_batch = HashMap::<u128, usize>::new();
        let mut projected_next_id = self.next_transaction_id;
        // Overlay only keys touched by this batch; cloning the journal's full
        // latest-key map on every fsync would scale with lifetime world edits.
        let mut projected_latest = HashMap::<StateKey, Vec<u8>>::new();
        for request in &requests {
            let tx = match request.transaction.clone().canonicalize() {
                Ok(tx) => tx,
                Err(error) => {
                    statuses.push(Status::Error(error.kind(), error.to_string()));
                    continue;
                }
            };
            if let Some(record) = self.known.get(&tx.id) {
                if self.records[record.index] == tx {
                    statuses.push(Status::Existing(CommitReceipt {
                        id: tx.id,
                        sequence: record.sequence,
                        duplicate: true,
                    }));
                } else {
                    statuses.push(Status::Error(
                        io::ErrorKind::AlreadyExists,
                        "journal transaction ID reused with different data".into(),
                    ));
                }
                continue;
            }
            if self.manifest.is_some() && tx.id < projected_next_id {
                statuses.push(Status::Error(
                    io::ErrorKind::AlreadyExists,
                    "journal transaction ID is below the generation watermark".into(),
                ));
                continue;
            }
            if let Some(&index) = in_batch.get(&tx.id) {
                if candidates[index] == tx {
                    statuses.push(Status::Repeated(index));
                } else {
                    statuses.push(Status::Error(
                        io::ErrorKind::AlreadyExists,
                        "journal transaction ID repeated with different data in batch".into(),
                    ));
                }
                continue;
            }
            if tx.changes.iter().any(|change| {
                projected_latest
                    .get(&change.key)
                    .or_else(|| self.latest.get(&change.key))
                    .is_some_and(|after| after != &change.before)
            }) {
                statuses.push(Status::Error(
                    io::ErrorKind::InvalidData,
                    "journal state-key before value breaks committed history".into(),
                ));
                continue;
            }
            for change in &tx.changes {
                projected_latest.insert(change.key.clone(), change.after.clone());
            }
            if self.manifest.is_some() {
                projected_next_id = tx.id.checked_add(1).unwrap_or(u128::MAX);
            }
            let index = candidates.len();
            in_batch.insert(tx.id, index);
            candidates.push(tx);
            statuses.push(Status::Candidate(index));
        }

        if candidates.is_empty() {
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(invalid_data("internal journal batch state"))
                    }
                })
                .collect();
        }

        let first_sequence = match self.physical_records.checked_add(1) {
            Some(sequence) => sequence,
            None => {
                return statuses
                    .into_iter()
                    .map(|status| match status {
                        Status::Existing(receipt) => Ok(receipt),
                        Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                        Status::Candidate(_) | Status::Repeated(_) => {
                            Err(invalid_data("journal sequence exhausted"))
                        }
                    })
                    .collect();
            }
        };
        if self
            .physical_records
            .checked_add(candidates.len() as u64)
            .is_none()
        {
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(invalid_data("journal sequence exhausted"))
                    }
                })
                .collect();
        }

        let mut next_transaction_id = self.next_transaction_id;
        for transaction in &candidates {
            let Some(next) = transaction.id.checked_add(1) else {
                return statuses
                    .into_iter()
                    .map(|status| match status {
                        Status::Existing(receipt) => Ok(receipt),
                        Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                        Status::Candidate(_) | Status::Repeated(_) => {
                            Err(invalid_data("journal transaction ID space exhausted"))
                        }
                    })
                    .collect();
            };
            next_transaction_id = next_transaction_id.max(next);
        }

        let mut frames = Vec::with_capacity(candidates.len());
        for transaction in &candidates {
            match encode_frame(transaction) {
                Ok(frame) => frames.push(frame),
                Err(error) => {
                    return statuses
                        .into_iter()
                        .map(|status| match status {
                            Status::Existing(receipt) => Ok(receipt),
                            Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                            Status::Candidate(_) | Status::Repeated(_) => {
                                Err(io::Error::new(error.kind(), error.to_string()))
                            }
                        })
                        .collect();
                }
            }
        }

        let append_bytes = frames
            .iter()
            .try_fold(0u64, |total, frame| total.checked_add(frame.len() as u64));
        let Some(projected_bytes) =
            append_bytes.and_then(|bytes| self.log_bytes.checked_add(bytes))
        else {
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(io::Error::other("journal byte length overflow"))
                    }
                })
                .collect();
        };
        if projected_bytes > MAX_JOURNAL_BYTES {
            let message = format!(
                "journal capacity reached ({} of {} bytes); checkpoint rotation is required",
                self.log_bytes, MAX_JOURNAL_BYTES
            );
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(io::Error::other(message.clone()))
                    }
                })
                .collect();
        }

        let append_result = (|| {
            for frame in &frames {
                self.file.write_all(frame)?;
            }
            self.file.sync_all()
        })();
        if let Err(error) = append_result {
            self.poisoned = Some((error.kind(), error.to_string()));
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(io::Error::new(error.kind(), error.to_string()))
                    }
                })
                .collect();
        }

        for (index, transaction) in candidates.into_iter().enumerate() {
            let sequence = first_sequence + index as u64;
            let record_index = self.records.len();
            self.known.insert(
                transaction.id,
                KnownRecord {
                    index: record_index,
                    sequence,
                },
            );
            self.records.push(transaction);
        }
        self.physical_records += frames.len() as u64;
        self.log_bytes = projected_bytes;
        self.next_transaction_id = next_transaction_id;
        self.latest.extend(projected_latest);
        let first_record = self.records.len() - frames.len();
        for (record_offset, transaction) in self.records[first_record..].iter().enumerate() {
            for (change_index, change) in transaction.changes.iter().enumerate() {
                self.history
                    .entry(change.key.clone())
                    .or_default()
                    .push((first_record + record_offset, change_index));
            }
        }

        statuses
            .into_iter()
            .map(|status| match status {
                Status::Existing(receipt) => Ok(receipt),
                Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                Status::Candidate(index) => Ok(CommitReceipt {
                    id: self.records[self.records.len() - frames.len() + index].id,
                    sequence: first_sequence + index as u64,
                    duplicate: false,
                }),
                Status::Repeated(index) => Ok(CommitReceipt {
                    id: self.records[self.records.len() - frames.len() + index].id,
                    sequence: first_sequence + index as u64,
                    duplicate: true,
                }),
            })
            .collect()
    }

    fn rotate(&mut self, expected_sequence: u64) -> io::Result<RotationReceipt> {
        self.rotate_using(expected_sequence, |path, manifest, cut, next_id, values| {
            rotation::rotate(path, manifest, cut, next_id, values)
        })
    }

    #[cfg(test)]
    fn rotate_crashing_at(
        &mut self,
        expected_sequence: u64,
        point: rotation::CrashPoint,
    ) -> io::Result<RotationReceipt> {
        self.rotate_using(expected_sequence, |path, manifest, cut, next_id, values| {
            rotation::rotate_crashing_at(path, manifest, cut, next_id, values, point)
        })
    }

    fn rotate_using(
        &mut self,
        expected_sequence: u64,
        switch: impl FnOnce(
            &Path,
            Option<&rotation::Manifest>,
            u64,
            u128,
            &HashMap<StateKey, Vec<u8>>,
        ) -> io::Result<rotation::SwitchedGeneration>,
    ) -> io::Result<RotationReceipt> {
        if let Some((kind, message)) = &self.poisoned {
            return Err(io::Error::new(*kind, message.clone()));
        }
        if expected_sequence != self.physical_records {
            return Err(invalid_data(
                "journal rotation sequence does not match durable sequence",
            ));
        }

        let generation = match switch(
            &self.path,
            self.manifest.as_ref(),
            expected_sequence,
            self.next_transaction_id,
            &self.latest,
        ) {
            Ok(switched) => switched,
            Err(error) => {
                // A failed directory sync may leave the new manifest visible.
                // Stop writes until reopen resolves the authoritative generation.
                self.poisoned = Some((error.kind(), error.to_string()));
                return Err(error);
            }
        };

        let old_file = std::mem::replace(&mut self.file, generation.file);
        self.generation = generation.manifest.generation;
        self.manifest = Some(generation.manifest);
        self.base_anchor.clone_from(&self.latest);
        self.records.clear();
        self.known.clear();
        self.history.clear();
        self.log_bytes = generation.tail_bytes;
        drop(old_file);
        rotation::cleanup_old_files(&self.path, &generation.old_files);
        Ok(RotationReceipt {
            cut_sequence: expected_sequence,
            generation: self.generation,
        })
    }
}

/// Dedicated append/fsync worker with nonblocking bounded submissions.
pub struct JournalWriter {
    sender: Option<SyncSender<WriterCommand>>,
    worker: Option<JoinHandle<io::Result<()>>>,
    usage: Arc<AtomicU64>,
    sequence: Arc<AtomicU64>,
}

/// Durable checkpoint boundary completed by an explicit rotation request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotationReceipt {
    /// Global sequence that was placed in the immutable base generation.
    pub cut_sequence: u64,
    /// Generation selected by the durable manifest switch.
    pub generation: u64,
}

/// Nonblocking failure to enqueue a rotation control command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotateError {
    Full,
    Closed,
}

impl std::fmt::Display for RotateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => formatter.write_str("journal request queue is full"),
            Self::Closed => formatter.write_str("journal writer is closed"),
        }
    }
}

impl std::error::Error for RotateError {}

impl JournalWriter {
    /// Queues a transaction without waiting. Poll or receive the returned
    /// channel away from the simulation's action path before applying/acking it.
    pub fn try_submit(
        &self,
        transaction: Transaction,
    ) -> Result<Receiver<io::Result<CommitReceipt>>, SubmitError> {
        let transaction = transaction.canonicalize().map_err(SubmitError::Invalid)?;
        let (acknowledge, receiver) = mpsc::channel();
        let request = WriterCommand::Append(Request {
            transaction,
            acknowledge,
        });
        let sender = self.sender.as_ref().ok_or(SubmitError::Closed)?;
        match sender.try_send(request) {
            Ok(()) => Ok(receiver),
            Err(TrySendError::Full(_)) => Err(SubmitError::Full),
            Err(TrySendError::Disconnected(_)) => Err(SubmitError::Closed),
        }
    }

    /// Current append-only file size, updated by the worker after each batch.
    pub fn bytes(&self) -> u64 {
        self.usage.load(Ordering::Acquire)
    }

    /// Current globally synced physical record sequence. Queued, unsynced
    /// submissions are deliberately not reflected here.
    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::Acquire)
    }

    /// Whether the active WAL tail has reached the checkpoint rotation target.
    /// This is advisory; append requests still fail closed at the hard cap.
    pub fn needs_rotation(&self) -> bool {
        self.bytes() >= JOURNAL_ROTATION_SOFT_LIMIT_BYTES
    }

    /// Queues an explicit generation switch after commands already submitted
    /// to this writer. The caller must freeze durable admissions, drain/apply
    /// commit receipts through `expected_sequence`, then drain successful
    /// BGED/BGIN/BGDP checkpoint receipts for that exact state before calling.
    /// Checkpointing and this request must stay off the tick's blocking path.
    /// A sequence mismatch is reported through the returned receiver.
    pub fn try_rotate(
        &self,
        expected_sequence: u64,
    ) -> Result<Receiver<io::Result<RotationReceipt>>, RotateError> {
        let (acknowledge, receiver) = mpsc::channel();
        let command = WriterCommand::Rotate {
            expected_sequence,
            acknowledge,
        };
        let sender = self.sender.as_ref().ok_or(RotateError::Closed)?;
        match sender.try_send(command) {
            Ok(()) => Ok(receiver),
            Err(TrySendError::Full(_)) => Err(RotateError::Full),
            Err(TrySendError::Disconnected(_)) => Err(RotateError::Closed),
        }
    }

    /// Closes the request queue, drains submitted work, and joins the writer.
    pub fn shutdown(mut self) -> io::Result<()> {
        self.sender.take();
        self.join_worker()
    }

    fn join_worker(&mut self) -> io::Result<()> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        match worker.join() {
            Ok(result) => result,
            Err(_) => Err(io::Error::other("journal worker thread panicked")),
        }
    }
}

impl Drop for JournalWriter {
    fn drop(&mut self) {
        self.sender.take();
        let _ = self.join_worker();
    }
}

struct Request {
    transaction: Transaction,
    acknowledge: mpsc::Sender<io::Result<CommitReceipt>>,
}

enum WriterCommand {
    Append(Request),
    Rotate {
        expected_sequence: u64,
        acknowledge: mpsc::Sender<io::Result<RotationReceipt>>,
    },
}

fn writer_loop(
    mut journal: Journal,
    requests: Receiver<WriterCommand>,
    batch_delay: Duration,
    usage: Arc<AtomicU64>,
    sequence: Arc<AtomicU64>,
) -> io::Result<()> {
    let mut pending_rotation = None;
    loop {
        let command = match pending_rotation.take().or_else(|| requests.recv().ok()) {
            Some(command) => command,
            None => return Ok(()),
        };
        let first = match command {
            WriterCommand::Append(request) => request,
            WriterCommand::Rotate {
                expected_sequence,
                acknowledge,
            } => {
                let result = journal.rotate(expected_sequence);
                usage.store(journal.bytes(), Ordering::Release);
                sequence.store(journal.sequence(), Ordering::Release);
                let _ = acknowledge.send(result);
                continue;
            }
        };
        let deadline = Instant::now() + batch_delay.min(Duration::from_millis(250));
        let mut batch = vec![first];
        while batch.len() < MAX_BATCH_RECORDS {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match requests.recv_timeout(deadline.saturating_duration_since(now)) {
                Ok(WriterCommand::Append(request)) => batch.push(request),
                Ok(rotation @ WriterCommand::Rotate { .. }) => {
                    pending_rotation = Some(rotation);
                    break;
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        // Catch requests already queued if the delay is zero or elapsed during
        // a wakeup. The bound prevents an unending stream starving acknowledgments.
        while batch.len() < MAX_BATCH_RECORDS {
            match requests.try_recv() {
                Ok(WriterCommand::Append(request)) => batch.push(request),
                Ok(rotation @ WriterCommand::Rotate { .. }) => {
                    pending_rotation = Some(rotation);
                    break;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        let acknowledgments = batch
            .iter()
            .map(|request| request.acknowledge.clone())
            .collect::<Vec<_>>();
        let results = journal.append_batch(batch);
        usage.store(journal.bytes(), Ordering::Release);
        sequence.store(journal.sequence(), Ordering::Release);
        for (acknowledge, result) in acknowledgments.into_iter().zip(results) {
            let _ = acknowledge.send(result);
        }
    }
}

fn validate_transaction(transaction: &Transaction, require_sorted: bool) -> io::Result<()> {
    if transaction.id == 0 {
        return Err(invalid_input("transaction ID zero is reserved"));
    }
    if transaction.changes.is_empty() || transaction.changes.len() > MAX_CHANGES {
        return Err(invalid_input("invalid journal change count"));
    }
    let mut previous: Option<&StateKey> = None;
    let mut total = 2usize + 16 + 8 + 4;
    for change in &transaction.changes {
        validate_key(&change.key)?;
        if change.before == change.after {
            return Err(invalid_input(
                "journal change has identical before and after values",
            ));
        }
        if let Some(prior) = previous {
            match prior.cmp(&change.key) {
                std::cmp::Ordering::Less => {}
                std::cmp::Ordering::Equal => {
                    return Err(invalid_input("duplicate journal state key"));
                }
                std::cmp::Ordering::Greater if require_sorted => {
                    return Err(invalid_input("journal state keys are not canonical"));
                }
                std::cmp::Ordering::Greater => {}
            }
        }
        previous = Some(&change.key);
        total = total
            .checked_add(1 + change.key.domain.len() + 4 + change.key.bytes.len() + 4)
            .and_then(|sum| sum.checked_add(change.before.len()))
            .and_then(|sum| sum.checked_add(4))
            .and_then(|sum| sum.checked_add(change.after.len()))
            .ok_or_else(|| invalid_input("journal transaction length overflow"))?;
        if total > MAX_RECORD_BYTES {
            return Err(invalid_input("journal transaction exceeds size limit"));
        }
    }
    Ok(())
}

fn validate_key(key: &StateKey) -> io::Result<()> {
    let mut namespace = key.domain.split(':');
    let valid_component = |component: &str| {
        !component.is_empty()
            && component.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
    };
    let namespaced = namespace.next().is_some_and(valid_component)
        && namespace.next().is_some_and(valid_component)
        && namespace.next().is_none();
    if key.domain.len() > MAX_DOMAIN_BYTES || !namespaced {
        return Err(invalid_input("invalid journal key domain"));
    }
    if key.bytes.len() > MAX_KEY_BYTES {
        return Err(invalid_input("journal key exceeds size limit"));
    }
    Ok(())
}

fn encode_frame(transaction: &Transaction) -> io::Result<Vec<u8>> {
    validate_transaction(transaction, true)?;
    let mut payload = Vec::new();
    payload.extend_from_slice(&RECORD_VERSION.to_le_bytes());
    payload.extend_from_slice(&transaction.id.to_le_bytes());
    payload.extend_from_slice(&transaction.tick.to_le_bytes());
    payload.extend_from_slice(&(transaction.changes.len() as u32).to_le_bytes());
    for change in &transaction.changes {
        payload.push(change.key.domain.len() as u8);
        payload.extend_from_slice(change.key.domain.as_bytes());
        payload.extend_from_slice(&(change.key.bytes.len() as u32).to_le_bytes());
        payload.extend_from_slice(&(change.before.len() as u32).to_le_bytes());
        payload.extend_from_slice(&(change.after.len() as u32).to_le_bytes());
        payload.extend_from_slice(&change.key.bytes);
        payload.extend_from_slice(&change.before);
        payload.extend_from_slice(&change.after);
    }
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| invalid_input("journal transaction exceeds size limit"))?;
    let length_bytes = payload_len.to_le_bytes();
    let checksum = frame_checksum(&length_bytes, &payload);
    let mut frame = Vec::with_capacity(FRAME_OVERHEAD + payload.len());
    frame.extend_from_slice(&length_bytes);
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(&checksum.to_le_bytes());
    Ok(frame)
}

fn decode_transaction(payload: &[u8]) -> io::Result<Transaction> {
    if payload.len() > MAX_RECORD_BYTES {
        return Err(invalid_data("journal record exceeds size limit"));
    }
    let mut reader = Reader::new(payload);
    if reader.u16()? != RECORD_VERSION {
        return Err(invalid_data("unsupported journal record version"));
    }
    let id = reader.u128()?;
    let tick = reader.u64()?;
    let count = reader.u32()? as usize;
    if count == 0 || count > MAX_CHANGES {
        return Err(invalid_data("invalid journal change count"));
    }
    let mut changes = Vec::with_capacity(count);
    for _ in 0..count {
        let domain_len = reader.u8()? as usize;
        let domain = std::str::from_utf8(reader.take(domain_len)?)
            .map_err(|_| invalid_data("journal key domain is not UTF-8"))?
            .to_owned();
        let key_len = reader.u32()? as usize;
        let before_len = reader.u32()? as usize;
        let after_len = reader.u32()? as usize;
        let key = reader.take(key_len)?.to_vec();
        let before = reader.take(before_len)?.to_vec();
        let after = reader.take(after_len)?.to_vec();
        changes.push(Change {
            key: StateKey { domain, bytes: key },
            before,
            after,
        });
    }
    if !reader.is_empty() {
        return Err(invalid_data("trailing bytes in journal record"));
    }
    let transaction = Transaction { id, tick, changes };
    validate_transaction(&transaction, true)
        .map_err(|error| invalid_data_owned(format!("invalid journal transaction: {error}")))?;
    Ok(transaction)
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| invalid_data("journal field length overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| invalid_data("truncated journal record payload"))?;
        self.offset = end;
        Ok(value)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("u16")))
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("u32")))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("u64")))
    }
    fn u128(&mut self) -> io::Result<u128> {
        Ok(u128::from_le_bytes(
            self.take(16)?.try_into().expect("u128"),
        ))
    }
    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn frame_checksum(length: &[u8; 4], payload: &[u8]) -> u32 {
    let mut bytes = Vec::with_capacity(length.len() + payload.len());
    bytes.extend_from_slice(length);
    bytes.extend_from_slice(payload);
    crc32(&bytes)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn truncate_tail(file: &mut File, offset: u64) -> io::Result<()> {
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
            != crc32(&header[..6])
    {
        return Err(invalid_data("invalid or unsupported journal header"));
    }
    Ok(())
}

fn legacy_header() -> [u8; FILE_HEADER_LEN] {
    let mut header = [0u8; FILE_HEADER_LEN];
    header[..4].copy_from_slice(FILE_MAGIC);
    header[4..6].copy_from_slice(&FILE_VERSION.to_le_bytes());
    let checksum = crc32(&header[..6]);
    header[6..].copy_from_slice(&checksum.to_le_bytes());
    header
}

fn write_legacy_header(file: &mut File) -> io::Result<()> {
    file.write_all(&legacy_header())?;
    file.sync_all()
}

fn sync_parent(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)?.sync_all()
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn invalid_data_owned(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn invalid_input(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
#[path = "journal/tests.rs"]
mod tests;

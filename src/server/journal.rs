//! Versioned write-ahead journal for durable server transactions.
//!
//! A transaction contains complete, opaque before/after values for every key it
//! changes. Encoding, recovery, writer concurrency, and generation rotation are
//! isolated in focused child modules; this module owns the stable data model.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io;

mod append;
mod codec;
mod generation;
mod recovery;
mod rotation;
mod writer;

pub use writer::{JournalWriter, RotateError, RotationReceipt};

use codec::{Reader, crc32, invalid_data, invalid_data_owned, validate_key, validate_transaction};
#[cfg(test)]
use codec::{encode_frame, frame_len};
#[cfg(test)]
use recovery::legacy_header;
use recovery::sync_parent;
#[cfg(test)]
use writer::Request;

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

/// Maximum encoded transaction payload, excluding its length and checksum.
#[cfg(test)]
pub const MAX_TRANSACTION_BYTES: usize = MAX_RECORD_BYTES;

/// Stable identity of one server-owned value. `domain` is a short namespaced
/// kind such as `bloxgloom:chunk_snapshot`; `bytes` is its full key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateKey {
    pub domain: String,
    pub bytes: Vec<u8>,
}

/// Checkpointed drop state used to compact owner tombstones and spawn
/// positions at a generation boundary. This is built from the server-owned
/// drop store only after its BGDP checkpoint has been synced.
#[derive(Clone, Debug, Default)]
pub(crate) struct DropCompaction {
    pub(crate) drops: Vec<CompactedDrop>,
    pub(crate) next_id: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct CompactedDrop {
    pub(crate) id: u64,
    pub(crate) owner: Vec<u8>,
    pub(crate) position: [f32; 3],
}

impl DropCompaction {
    fn into_values(self) -> io::Result<HashMap<StateKey, Vec<u8>>> {
        if self.next_id == 0 {
            return Err(invalid_data("invalid compacted drop allocator"));
        }
        let mut values = HashMap::with_capacity(self.drops.len().saturating_mul(2) + 1);
        let mut max_id = 0;
        for drop in self.drops {
            if drop.id == 0
                || drop.id >= self.next_id
                || drop.owner.is_empty()
                || drop
                    .position
                    .iter()
                    .any(|coordinate| !coordinate.is_finite())
            {
                return Err(invalid_data("invalid compacted drop owner"));
            }
            max_id = max_id.max(drop.id);
            let owner_key = StateKey::new("bloxgloom:drop_owner", drop.id.to_le_bytes().to_vec());
            let position_key =
                StateKey::new("bloxgloom:drop_position", drop.id.to_le_bytes().to_vec());
            let mut position = Vec::with_capacity(12);
            for coordinate in drop.position {
                position.extend(coordinate.to_le_bytes());
            }
            if values.insert(owner_key, drop.owner).is_some()
                || values.insert(position_key, position).is_some()
            {
                return Err(invalid_data("duplicate compacted drop ID"));
            }
        }
        if self.next_id <= max_id {
            return Err(invalid_data("compacted drop allocator is behind live IDs"));
        }
        values.insert(
            StateKey::new("bloxgloom:drop_allocator", Vec::new()),
            self.next_id.to_le_bytes().to_vec(),
        );
        Ok(values)
    }
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

    /// Idempotently reapplies this transaction's exact `after` values. All
    /// keys are checked before the first write, and a partially completed set
    /// can safely be retried after a crash.
    #[cfg(test)]
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
    drop_owner_set_closed: bool,
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
    /// Unique transactions in current-tail log order. A rotated base has
    /// already materialized all state at its cut, so only its tail is returned.
    /// Exact duplicate IDs are represented once; conflicts fail `open`.
    #[cfg(test)]
    pub fn records(&self) -> &[Transaction] {
        &self.records
    }

    /// Next nonzero monotonically increasing ID for a server-owned action.
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

    /// Final committed value for every base or tail key.
    pub fn latest_values(&self) -> BTreeMap<StateKey, Vec<u8>> {
        self.latest
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    /// Whether the selected base proves that every drop ID in its checkpoint
    /// was materialized in the journal. Legacy WALs and version-1 bases do not
    /// carry this guarantee.
    pub fn drop_owner_set_closed(&self) -> bool {
        self.drop_owner_set_closed
    }

    /// Replays each unique transaction in the current tail in log order.
    #[cfg(test)]
    pub fn replay(&self, mut apply: impl FnMut(&Transaction) -> io::Result<()>) -> io::Result<()> {
        for record in &self.records {
            apply(record)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

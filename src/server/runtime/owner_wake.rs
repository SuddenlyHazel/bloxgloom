//! Durable pending-wake flags for owner-system effect destinations.
//!
//! THE RULE (see `owner_effects`): an effect may only cause work to happen
//! SOONER. The durable truth is therefore the schedule, not the effect: a
//! pending wake is a tiny bounded "this owner is due" flag keyed by
//! destination, carrying no effect payload. The destination still does its own
//! durable work through its normal handler when the wake is served, so a lost
//! wake costs latency and never state.
//!
//! Wake flags live in the main journal under [`OWNER_WAKE_DOMAIN`] — new
//! persisted state alongside `bloxgloom:owner_state`, with existing encodings
//! untouched. A present value means the destination is due; an empty `after`
//! value clears the flag. The producing tick rides in the envelope so replays
//! stay deterministic; it is informational only and never schedules anything
//! by itself.
//!
//! Corruption (bad magic, version, length, checksum) reports `InvalidData` so
//! the coordinator can stop. Capacity (too many pending wakes) reports
//! `WouldBlock`: the producing wave defers and retries, and nothing commits.

use super::super::journal::{Change, StateKey};
use super::super::parallel::OwnerKey;
use super::super::registry::SystemId;
use crate::world::ChunkKey;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};

/// Journal domain for durable pending owner wakes. New persisted state; no
/// existing encoding uses it.
pub(in crate::server) const OWNER_WAKE_DOMAIN: &str = "bloxgloom:owner_wake";

/// Magic for the wake-flag value envelope: `wake tick + checksum`. The
/// envelope is new persisted state.
const OWNER_WAKE_MAGIC: &[u8; 4] = b"BGWK";
const OWNER_WAKE_VERSION: u16 = 1;

/// Exact encoded length of one present wake flag: magic + version + tick +
/// checksum. Fixed and tiny by construction; there is nothing to truncate.
pub(in crate::server) const OWNER_WAKE_VALUE_LEN: usize = 4 + 2 + 8 + 4;

/// Stable journal address for one destination's pending wake. The byte layout
/// mirrors [`super::owner_codec::owner_state_key`] so the same destination
/// maps to the same suffix in either domain; the domains never collide.
pub(in crate::server) fn owner_wake_key(system: &SystemId, owner: OwnerKey) -> StateKey {
    let system_bytes = system.as_str().as_bytes();
    let mut bytes = Vec::with_capacity(2 + system_bytes.len() + 1 + 32);
    let system_len = u16::try_from(system_bytes.len()).expect("system id fits in u16");
    bytes.extend(system_len.to_le_bytes());
    bytes.extend(system_bytes);
    match owner {
        OwnerKey::Chunk(key) => {
            bytes.push(0);
            bytes.extend(key.x.to_le_bytes());
            bytes.extend(key.y.to_le_bytes());
            bytes.extend(key.z.to_le_bytes());
        }
        OwnerKey::Entity(id) => {
            bytes.push(1);
            bytes.extend(id.to_le_bytes());
        }
        OwnerKey::Profile(id) => {
            bytes.push(2);
            bytes.extend(id.to_le_bytes());
        }
    }
    StateKey::new(OWNER_WAKE_DOMAIN, bytes)
}

/// Decodes the system identity and owner from a wake-flag key. Returns `None`
/// for malformed keys so recovery can fail closed with `InvalidData`.
pub(in crate::server) fn decode_owner_wake_key(key: &StateKey) -> Option<(String, OwnerKey)> {
    if key.domain != OWNER_WAKE_DOMAIN || key.bytes.len() < 3 {
        return None;
    }
    let system_len = usize::from(u16::from_le_bytes(key.bytes[0..2].try_into().ok()?));
    let system_end = 2usize.checked_add(system_len)?;
    let tag_index = system_end;
    let system_bytes = key.bytes.get(2..system_end)?;
    let system = std::str::from_utf8(system_bytes).ok()?.to_owned();
    let tag = *key.bytes.get(tag_index)?;
    let rest = key.bytes.get(tag_index + 1..)?;
    let owner = match tag {
        0 => {
            if rest.len() != 12 {
                return None;
            }
            OwnerKey::Chunk(ChunkKey {
                x: i32::from_le_bytes(rest[0..4].try_into().ok()?),
                y: i32::from_le_bytes(rest[4..8].try_into().ok()?),
                z: i32::from_le_bytes(rest[8..12].try_into().ok()?),
            })
        }
        1 => {
            if rest.len() != 8 {
                return None;
            }
            OwnerKey::Entity(u64::from_le_bytes(rest.try_into().ok()?))
        }
        2 => {
            if rest.len() != 16 {
                return None;
            }
            OwnerKey::Profile(u128::from_le_bytes(rest.try_into().ok()?))
        }
        _ => return None,
    };
    SystemId::new(&system).ok()?;
    Some((system, owner))
}

/// Encodes a present wake flag carrying the producing tick.
pub(in crate::server) fn encode_wake_value(wake_tick: u64) -> Vec<u8> {
    let mut value = Vec::with_capacity(OWNER_WAKE_VALUE_LEN);
    value.extend(OWNER_WAKE_MAGIC);
    value.extend(OWNER_WAKE_VERSION.to_le_bytes());
    value.extend(wake_tick.to_le_bytes());
    let crc = crc32(&value);
    value.extend(crc.to_le_bytes());
    debug_assert_eq!(value.len(), OWNER_WAKE_VALUE_LEN);
    value
}

/// Decodes a present wake flag into its producing tick. An empty value is a
/// cleared flag, not a present one — callers treat absence as absent; this
/// reports it as `InvalidData` so a cleared flag can never be mistaken for a
/// due destination. Any structural problem is `InvalidData`: genuine
/// corruption that may stop the coordinator. Capacity pressure is never
/// reported through this path.
pub(in crate::server) fn decode_wake_value(value: &[u8]) -> io::Result<u64> {
    if value.len() != OWNER_WAKE_VALUE_LEN {
        return Err(invalid_data("owner wake value has a bad length"));
    }
    let (body, crc_bytes) = value.split_at(value.len() - 4);
    let expected = u32::from_le_bytes(crc_bytes.try_into().expect("crc is 4 bytes"));
    if crc32(body) != expected {
        return Err(invalid_data("owner wake value checksum mismatch"));
    }
    if &body[0..4] != OWNER_WAKE_MAGIC {
        return Err(invalid_data("owner wake value has a bad magic"));
    }
    if u16::from_le_bytes(body[4..6].try_into().expect("version is 2 bytes")) != OWNER_WAKE_VERSION
    {
        return Err(invalid_data("owner wake value has an unsupported version"));
    }
    Ok(u64::from_le_bytes(
        body[6..14].try_into().expect("wake tick is 8 bytes"),
    ))
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = crc & 1;
            crc >>= 1;
            if mask == 1 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}

/// Barrier-owned pending-wake set for owner-system effect destinations.
///
/// `pending` is the WAL-backed truth: every entry has a receipted record and
/// survives restart through [`PendingWakeStore::recover`]. `staged` holds
/// flags prepared for a wave whose record has not been receipted yet, so a
/// second prepare for the same destination chains onto the staged flag
/// instead of staging a duplicate key (the journal rejects duplicate keys
/// within one record). Staged flags become visible only at
/// [`PendingWakeStore::commit_sets`], after the WAL receipt; a wave that
/// defers before its receipt calls [`PendingWakeStore::cancel_sets`] so a
/// retry re-stages from the WAL-backed state.
///
/// Serving (and clearing) a wake is a later wiring step; this store already
/// applies clears in [`PendingWakeStore::apply_replayed`] so receipted clear
/// records converge.
pub(in crate::server) struct PendingWakeStore {
    pending: BTreeMap<(SystemId, OwnerKey), u64>,
    staged: BTreeMap<(SystemId, OwnerKey), u64>,
    /// Same-process publication barrier only, bounded by pending flags. WAL
    /// wake ticks are informational across restart, not deadlines. Recovered
    /// flags have no publication fence and can be served immediately.
    published: BTreeMap<(SystemId, OwnerKey), u64>,
}

impl std::fmt::Debug for PendingWakeStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingWakeStore")
            .field("pending", &self.pending.len())
            .field("staged", &self.staged.len())
            .finish()
    }
}

impl PendingWakeStore {
    pub fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            staged: BTreeMap::new(),
            published: BTreeMap::new(),
        }
    }

    /// Rebuilds the wake set from journal latest-values. Unknown domains are
    /// skipped by the caller contract (only this domain's entries are passed
    /// here in practice; others are ignored). Malformed keys, undecodable
    /// values, and duplicate keys are `InvalidData`: the save cannot run.
    /// Empty values are cleared-flag tombstones and are skipped.
    pub fn recover(latest: &BTreeMap<StateKey, Vec<u8>>) -> io::Result<Self> {
        let mut store = Self::new();
        for (key, value) in latest {
            if key.domain != OWNER_WAKE_DOMAIN {
                continue;
            }
            let Some((system_name, owner)) = decode_owner_wake_key(key) else {
                return Err(invalid_data("owner wake key is malformed"));
            };
            let system = SystemId::new(system_name)
                .map_err(|_| invalid_data("owner wake key has a bad system id"))?;
            if value.is_empty() {
                continue;
            }
            let tick = decode_wake_value(value)?;
            if store.pending.insert((system, owner), tick).is_some() {
                return Err(invalid_data("duplicate owner wake key"));
            }
        }
        Ok(store)
    }

    /// Receipted flag count. Test builds use this to assert staging,
    /// recovery, and clearing without exposing the set itself.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Flagged destinations for one system in canonical order, with their
    /// producing ticks. The caller serves only those with live cells; flags
    /// for still-absent owners stay held.
    #[cfg(test)]
    pub fn flagged_for(&self, system: &SystemId) -> Vec<(OwnerKey, u64)> {
        self.pending
            .iter()
            .filter(|((candidate, _), _)| candidate == system)
            .map(|((_, owner), tick)| (*owner, *tick))
            .collect()
    }

    /// Bounded rotating inspection, including unloaded destinations. The
    /// caller advances the cursor past inspected flags so absent low keys do
    /// not hide loaded higher keys on every wave.
    pub fn flagged_from(
        &self,
        system: &SystemId,
        cursor: Option<OwnerKey>,
        limit: usize,
    ) -> Vec<(OwnerKey, u64)> {
        if limit == 0 {
            return Vec::new();
        }
        let first = OwnerKey::Chunk(crate::world::ChunkKey {
            x: i32::MIN,
            y: i32::MIN,
            z: i32::MIN,
        });
        let start = cursor.unwrap_or(first);
        let bound = if cursor.is_some() {
            std::ops::Bound::Excluded((system.clone(), start))
        } else {
            std::ops::Bound::Included((system.clone(), start))
        };
        let after = self
            .pending
            .range((bound, std::ops::Bound::Unbounded))
            .take_while(|((id, _), _)| id == system);
        let before = self
            .pending
            .range((system.clone(), first)..=(system.clone(), start))
            .take(if cursor.is_some() { limit } else { 0 });
        after
            .chain(before)
            .take(limit)
            .map(|((_, owner), tick)| (*owner, *tick))
            .collect()
    }

    /// Stages clear records for served destinations. Absent flags stage
    /// nothing and intra-wave duplicates collapse, so clearing is idempotent
    /// and no key repeats within one record. Staging mutates nothing: flags
    /// leave the set only at [`PendingWakeStore::commit_clears`], after the
    /// carrying record's receipt, so a deferred serve simply serves again.
    pub fn stage_clears(&self, served: &[(SystemId, OwnerKey)]) -> Vec<Change> {
        let mut seen = BTreeSet::new();
        let mut changes = Vec::new();
        for (system, owner) in served {
            let key = (system.clone(), *owner);
            if !seen.insert(key.clone()) {
                continue;
            }
            if let Some(tick) = self.pending.get(&key) {
                changes.push(Change::new(
                    owner_wake_key(system, *owner),
                    encode_wake_value(*tick),
                    Vec::new(),
                ));
            }
        }
        changes
    }

    /// Removes cleared flags after their carrying record's receipt. The
    /// record already holds the exact clear; this only drops the live set
    /// entries so a served flag is never served twice.
    pub fn commit_clears(&mut self, served: &[(SystemId, OwnerKey)]) {
        for (system, owner) in served {
            self.pending.remove(&(system.clone(), *owner));
            self.published.remove(&(system.clone(), *owner));
        }
    }

    pub fn published_at(&self, system: &SystemId, owner: OwnerKey) -> Option<u64> {
        self.published.get(&(system.clone(), owner)).copied()
    }

    /// Prepares set-flags for destinations with no live flag, chaining onto
    /// staged state so no key is staged twice. Destinations that already have
    /// a live or staged flag are skipped in place: a duplicate wake only
    /// re-asserts "due", never queues extra work. Exceeding `limit` live plus
    /// staged flags rejects the whole set with `WouldBlock` before anything
    /// is staged; the producing wave defers and retries.
    pub fn prepare_sets(
        &mut self,
        wakes: &[(SystemId, OwnerKey)],
        wake_tick: u64,
        limit: usize,
    ) -> io::Result<PreparedWakeSets> {
        let mut fresh = Vec::new();
        for (system, owner) in wakes {
            let key = (system.clone(), *owner);
            if self.pending.contains_key(&key) || self.staged.contains_key(&key) {
                continue;
            }
            // `staged` is keyed, so re-scanning `fresh` is the only way to
            // collapse intra-wave duplicates before the journal sees them.
            if fresh.contains(&key) {
                continue;
            }
            fresh.push(key);
        }
        if self.pending.len() + self.staged.len() + fresh.len() > limit {
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                format!(
                    "owner wake queue full: {} pending, {} staged, {} fresh, limit {limit}",
                    self.pending.len(),
                    self.staged.len(),
                    fresh.len()
                ),
            ));
        }
        let mut changes = Vec::with_capacity(fresh.len());
        for key in &fresh {
            let before = self
                .pending
                .get(key)
                .map(|tick| encode_wake_value(*tick))
                .unwrap_or_default();
            changes.push(Change::new(
                owner_wake_key(&key.0, key.1),
                before,
                encode_wake_value(wake_tick),
            ));
            self.staged.insert(key.clone(), wake_tick);
        }
        Ok(PreparedWakeSets {
            staged: fresh,
            changes,
        })
    }

    /// Makes prepared flags visible after their WAL receipt. The record
    /// already carried the exact values; this only moves staged entries into
    /// the WAL-backed set.
    pub fn commit_sets(&mut self, prepared: PreparedWakeSets) {
        for key in prepared.staged {
            if let Some(tick) = self.staged.remove(&key) {
                self.published.insert(key.clone(), tick);
                self.pending.insert(key, tick);
            }
        }
    }

    /// Withdraws prepared flags from a wave that deferred before its receipt,
    /// so a retry re-stages from the WAL-backed state.
    pub fn cancel_sets(&mut self, prepared: PreparedWakeSets) {
        for key in prepared.staged {
            self.staged.remove(&key);
        }
    }

    /// Applies already-committed wake-domain changes after their WAL receipt.
    /// Every before-value is rechecked against the live set (absent reads as
    /// empty, matching a fresh set), so a mismatch is genuine corruption and
    /// the coordinator must stop. An empty `after` clears the flag.
    ///
    /// Non-wake keys are ignored; the caller filters the transaction's change
    /// set to this domain.
    pub fn apply_replayed(&mut self, changes: &[Change]) -> io::Result<()> {
        for change in changes {
            if change.key.domain != OWNER_WAKE_DOMAIN {
                continue;
            }
            let Some((system_name, owner)) = decode_owner_wake_key(&change.key) else {
                return Err(invalid_data("owner wake key is malformed"));
            };
            let system = SystemId::new(system_name)
                .map_err(|_| invalid_data("owner wake key has a bad system id"))?;
            let key = (system, owner);
            let current = self
                .pending
                .get(&key)
                .map(|tick| encode_wake_value(*tick))
                .unwrap_or_default();
            if current != change.before {
                return Err(invalid_data("owner wake replay precondition mismatch"));
            }
            if change.after.is_empty() {
                self.pending.remove(&key);
                self.published.remove(&key);
                continue;
            }
            let tick = decode_wake_value(&change.after)?;
            self.published.insert(key.clone(), tick);
            self.pending.insert(key, tick);
        }
        Ok(())
    }
}

/// A validated set-flag batch. The changes borrow nothing: the caller submits
/// them with the producer wave's record, then calls `commit_sets` after the
/// receipt or `cancel_sets` if the wave defers.
#[derive(Debug)]
pub(in crate::server) struct PreparedWakeSets {
    staged: Vec<(SystemId, OwnerKey)>,
    changes: Vec<Change>,
}

impl PreparedWakeSets {
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }
}

#[cfg(test)]
#[path = "owner_wake/tests.rs"]
mod tests;

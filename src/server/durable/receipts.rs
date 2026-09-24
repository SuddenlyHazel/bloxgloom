//! Compact, checkpointed per-profile action history and replay floor.
//!
//! One WAL key contains the entire bounded window. The key is changed in the
//! same transaction as gameplay state, so retirement cannot lose the replay
//! floor or leave an action outcome without its world/inventory effects.

use super::StateKey;
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAGIC: &[u8; 4] = b"BGAR";
const VERSION: u16 = 1;
pub(super) const WINDOW: usize = 128;
const MAX_PAYLOAD: usize = 64;
const MAX_REASON: usize = 32;
const MAX_SNAPSHOT: usize = 64 + WINDOW * (8 + 1 + 1 + MAX_REASON + 1 + MAX_PAYLOAD);
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResultRecord {
    pub(super) payload: Vec<u8>,
    pub(super) accepted: bool,
    pub(super) reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::server) struct ReceiptLedger {
    pub(super) epoch: u64,
    pub(super) acknowledged: u64,
    pub(super) next_seq: u64,
    pub(super) results: VecDeque<ResultRecord>,
}

#[derive(Clone, Debug)]
pub(super) enum Admission {
    New,
    Replay(ResultRecord),
    Retired,
    WrongEpoch,
    Gap,
    Full,
}

#[derive(Clone, Debug)]
pub(super) enum ReceiptEvent {
    Result(ResultRecord),
    EpochGrant,
    Ack,
}

#[derive(Clone, Debug)]
pub(super) struct ReceiptTransition {
    pub(super) profile: u128,
    pub(super) before: Vec<u8>,
    pub(super) after: Vec<u8>,
    pub(super) ledger: ReceiptLedger,
    pub(super) event: ReceiptEvent,
}

impl ReceiptTransition {
    pub(super) fn new(
        profile: u128,
        before_ledger: &ReceiptLedger,
        ledger: ReceiptLedger,
        event: ReceiptEvent,
    ) -> io::Result<Self> {
        let before = if before_ledger.epoch == 0 {
            Vec::new()
        } else {
            before_ledger.encode()?
        };
        let after = ledger.encode()?;
        Ok(Self {
            profile,
            before,
            after,
            ledger,
            event,
        })
    }
}

pub(super) fn split_action_id(action_id: u128) -> (u64, u64) {
    ((action_id >> 64) as u64, action_id as u64)
}

impl ReceiptLedger {
    pub(in crate::server) fn current_epoch(&self) -> u64 {
        self.epoch
    }

    #[cfg(test)]
    pub(in crate::server) fn acknowledged_seq(&self) -> u64 {
        self.acknowledged
    }

    #[cfg(test)]
    pub(in crate::server) fn outstanding_len(&self) -> usize {
        self.results.len()
    }

    pub(super) fn admission(&self, action_id: u128, payload: &[u8]) -> Admission {
        let (epoch, seq) = split_action_id(action_id);
        if epoch == 0 || seq == 0 || epoch != self.epoch {
            return Admission::WrongEpoch;
        }
        if seq <= self.acknowledged {
            return Admission::Retired;
        }
        if seq < self.next_seq {
            let index = (seq - self.acknowledged - 1) as usize;
            return match self.results.get(index) {
                Some(record) if record.payload == payload => Admission::Replay(record.clone()),
                Some(_) => Admission::Retired,
                None => Admission::Gap,
            };
        }
        if seq > self.next_seq {
            return Admission::Gap;
        }
        if self.results.len() >= WINDOW || self.next_seq == u64::MAX {
            return Admission::Full;
        }
        Admission::New
    }

    pub(super) fn grant_next_epoch(&self) -> io::Result<Self> {
        // Called only while joining. The grant WAL receipt precedes Welcome;
        // Welcome starts an authoritative inventory/chunk resync. That durable
        // transition closes the former epoch, including unacknowledged results.
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or_else(|| invalid("action session epoch exhausted"))?;
        Ok(Self {
            epoch,
            acknowledged: 0,
            next_seq: 1,
            results: VecDeque::new(),
        })
    }

    pub(super) fn append_result(&self, record: ResultRecord) -> io::Result<Self> {
        if self.epoch == 0 || self.results.len() >= WINDOW {
            return Err(invalid("action result window full"));
        }
        validate_record(&record)?;
        let mut next = self.clone();
        next.next_seq = next
            .next_seq
            .checked_add(1)
            .ok_or_else(|| invalid("action sequence exhausted"))?;
        next.results.push_back(record);
        Ok(next)
    }

    pub(super) fn acknowledge(&self, epoch: u64, through_seq: u64) -> io::Result<Option<Self>> {
        if self.epoch == 0 || epoch != self.epoch || through_seq > self.next_seq.saturating_sub(1) {
            return Err(invalid("invalid action acknowledgement"));
        }
        if through_seq <= self.acknowledged {
            return Ok(None);
        }
        let mut next = self.clone();
        let retired = (through_seq - self.acknowledged) as usize;
        for _ in 0..retired {
            next.results
                .pop_front()
                .ok_or_else(|| invalid("action result gap"))?;
        }
        next.acknowledged = through_seq;
        Ok(Some(next))
    }

    pub(super) fn encode(&self) -> io::Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(64 + self.results.len() * 64);
        bytes.extend(MAGIC);
        bytes.extend(VERSION.to_le_bytes());
        bytes.extend(self.epoch.to_le_bytes());
        bytes.extend(self.acknowledged.to_le_bytes());
        bytes.extend(self.next_seq.to_le_bytes());
        bytes.extend((self.results.len() as u16).to_le_bytes());
        for record in &self.results {
            bytes.push(u8::from(record.accepted));
            bytes.push(record.reason.len() as u8);
            bytes.extend(record.reason.as_bytes());
            bytes.push(record.payload.len() as u8);
            bytes.extend(&record.payload);
        }
        bytes.extend(checksum(&bytes).to_le_bytes());
        Ok(bytes)
    }

    pub(super) fn decode(bytes: &[u8]) -> io::Result<Self> {
        if !(36..=MAX_SNAPSHOT).contains(&bytes.len())
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != VERSION
        {
            return Err(invalid("invalid action ledger snapshot"));
        }
        let check_at = bytes.len() - 4;
        if checksum(&bytes[..check_at]) != u32::from_le_bytes(bytes[check_at..].try_into().unwrap())
        {
            return Err(invalid("action ledger checksum mismatch"));
        }
        let epoch = u64::from_le_bytes(bytes[6..14].try_into().unwrap());
        let acknowledged = u64::from_le_bytes(bytes[14..22].try_into().unwrap());
        let next_seq = u64::from_le_bytes(bytes[22..30].try_into().unwrap());
        let count = u16::from_le_bytes(bytes[30..32].try_into().unwrap()) as usize;
        if count > WINDOW {
            return Err(invalid("action ledger window too large"));
        }
        let mut offset = 32;
        let mut results = VecDeque::with_capacity(count);
        for _ in 0..count {
            let accepted = match *bytes
                .get(offset)
                .ok_or_else(|| invalid("truncated action ledger"))?
            {
                0 => false,
                1 => true,
                _ => return Err(invalid("invalid action result flag")),
            };
            offset += 1;
            let reason_len = *bytes
                .get(offset)
                .ok_or_else(|| invalid("truncated action ledger"))?
                as usize;
            offset += 1;
            if reason_len > MAX_REASON || offset + reason_len > check_at {
                return Err(invalid("invalid action result reason"));
            }
            let reason = String::from_utf8(bytes[offset..offset + reason_len].to_vec())
                .map_err(|_| invalid("invalid action result UTF-8"))?;
            offset += reason_len;
            let payload_len = *bytes
                .get(offset)
                .ok_or_else(|| invalid("truncated action ledger"))?
                as usize;
            offset += 1;
            if payload_len == 0 || payload_len > MAX_PAYLOAD || offset + payload_len > check_at {
                return Err(invalid("invalid action payload length"));
            }
            let payload = bytes[offset..offset + payload_len].to_vec();
            offset += payload_len;
            let record = ResultRecord {
                payload,
                accepted,
                reason,
            };
            validate_record(&record)?;
            results.push_back(record);
        }
        if offset != check_at {
            return Err(invalid("trailing action ledger bytes"));
        }
        let ledger = Self {
            epoch,
            acknowledged,
            next_seq,
            results,
        };
        ledger.validate()?;
        Ok(ledger)
    }

    fn validate(&self) -> io::Result<()> {
        if self.epoch == 0
            || self.next_seq == 0
            || self.acknowledged >= self.next_seq
            || self.next_seq - self.acknowledged - 1 != self.results.len() as u64
            || self.results.len() > WINDOW
        {
            return Err(invalid("invalid action ledger frontier"));
        }
        for record in &self.results {
            validate_record(record)?;
        }
        Ok(())
    }
}

fn validate_record(record: &ResultRecord) -> io::Result<()> {
    if record.payload.is_empty()
        || record.payload.len() > MAX_PAYLOAD
        || record.reason.len() > MAX_REASON
        || (record.accepted && !record.reason.is_empty())
    {
        return Err(invalid("invalid action result record"));
    }
    Ok(())
}

#[derive(Clone)]
pub(super) struct ReceiptStore {
    root: PathBuf,
}

impl ReceiptStore {
    pub(super) fn new(world_dir: &Path) -> io::Result<Self> {
        let root = world_dir.join("receipts");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path(&self, profile: u128) -> PathBuf {
        self.root.join(format!("{profile:032x}.ledger"))
    }

    pub(super) fn read(&self, profile: u128) -> io::Result<Option<Vec<u8>>> {
        if profile == 0 {
            return Err(invalid("missing receipt profile"));
        }
        match File::open(self.path(profile)) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take((MAX_SNAPSHOT + 1) as u64)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > MAX_SNAPSHOT {
                    return Err(invalid("action ledger file too large"));
                }
                ReceiptLedger::decode(&bytes)?;
                Ok(Some(bytes))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) fn write(&self, profile: u128, bytes: &[u8]) -> io::Result<()> {
        if profile == 0 {
            return Err(invalid("missing receipt profile"));
        }
        ReceiptLedger::decode(bytes)?;
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
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, self.path(profile))?;
            File::open(&self.root)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub(super) fn validate_no_orphans(
        &self,
        latest: &std::collections::BTreeMap<StateKey, Vec<u8>>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(hex) = name.strip_suffix(".ledger") else {
                continue;
            };
            if hex.len() != 32 {
                return Err(invalid("invalid action ledger filename"));
            }
            let profile = u128::from_str_radix(hex, 16)
                .map_err(|_| invalid("invalid action ledger filename"))?;
            if !latest.contains_key(&state_key(profile)) {
                return Err(invalid("action ledger file has no journal frontier"));
            }
        }
        Ok(())
    }
}

pub(super) fn state_key(profile: u128) -> StateKey {
    StateKey::new("bloxgloom:action_ledger", profile.to_le_bytes().to_vec())
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
#[path = "receipts/tests.rs"]
mod tests;

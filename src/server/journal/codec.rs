//! Transaction validation, binary frame encoding, and checksums.

use super::{
    Change, MAX_CHANGES, MAX_DOMAIN_BYTES, MAX_KEY_BYTES, MAX_RECORD_BYTES, RECORD_VERSION,
    StateKey, Transaction,
};
use std::io;

pub(super) fn transaction_size(transaction: &Transaction) -> Option<usize> {
    transaction
        .changes
        .iter()
        .try_fold(2usize + 16 + 8 + 4, |total, change| {
            total
                .checked_add(1 + change.key.domain.len() + 4 + change.key.bytes.len() + 4)
                .and_then(|sum| sum.checked_add(change.before.len()))
                .and_then(|sum| sum.checked_add(4))
                .and_then(|sum| sum.checked_add(change.after.len()))
        })
}

pub(super) fn validate_transaction(
    transaction: &Transaction,
    require_sorted: bool,
) -> io::Result<()> {
    if transaction.id == 0 {
        return Err(invalid_input("transaction ID zero is reserved"));
    }
    if transaction.changes.is_empty() || transaction.changes.len() > MAX_CHANGES {
        return Err(invalid_input("invalid journal change count"));
    }
    let mut previous: Option<&StateKey> = None;
    let total = transaction_size(transaction)
        .ok_or_else(|| invalid_input("journal transaction length overflow"))?;
    if total > MAX_RECORD_BYTES {
        return Err(invalid_input("journal transaction exceeds size limit"));
    }
    for change in &transaction.changes {
        validate_key(&change.key)?;
        if change.key.domain == super::CLOCK_DOMAIN {
            return Err(invalid_input("journal clock metadata is reserved"));
        }
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
    }
    Ok(())
}

pub(super) fn validate_key(key: &StateKey) -> io::Result<()> {
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

/// Exact encoded frame length after transaction validation, without building
/// a second copy of a potentially one-megabyte payload on the tick thread.
pub(super) fn frame_len(transaction: &Transaction) -> u64 {
    let mut bytes = (super::FRAME_OVERHEAD + 2 + 16 + 8 + 4) as u64;
    for change in &transaction.changes {
        bytes += (1 + change.key.domain.len() + 4 + 4 + 4 + change.key.bytes.len()) as u64;
        bytes += (change.before.len() + change.after.len()) as u64;
    }
    bytes
}

pub(super) fn encode_frame(transaction: &Transaction) -> io::Result<Vec<u8>> {
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
    let mut frame = Vec::with_capacity(super::FRAME_OVERHEAD + payload.len());
    frame.extend_from_slice(&length_bytes);
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(&checksum.to_le_bytes());
    Ok(frame)
}

pub(super) fn decode_transaction(payload: &[u8]) -> io::Result<Transaction> {
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

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
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

    pub(super) fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("u16")))
    }

    pub(super) fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("u32")))
    }

    pub(super) fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("u64")))
    }

    pub(super) fn u128(&mut self) -> io::Result<u128> {
        Ok(u128::from_le_bytes(
            self.take(16)?.try_into().expect("u128"),
        ))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

pub(super) fn frame_checksum(length: &[u8; 4], payload: &[u8]) -> u32 {
    let mut bytes = Vec::with_capacity(length.len() + payload.len());
    bytes.extend_from_slice(length);
    bytes.extend_from_slice(payload);
    crc32(&bytes)
}

pub(super) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

pub(super) fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) fn invalid_data_owned(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) fn invalid_input(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

//! Tagged mailbox keys leave ordinary owner-wake encodings unchanged.
use super::*;
use crate::server::runtime::owner_wake::{
    OWNER_WAKE_DOMAIN, crc32, decode_owner_wake_key, owner_wake_key,
};
use bloxgloom_host_api::system::{IntentId, MAX_INTENT_PAYLOAD_BYTES, Owner};

pub(in crate::server) fn is_key(key: &StateKey) -> bool {
    key.domain == OWNER_WAKE_DOMAIN && key.bytes.starts_with(&[0, 0])
}
pub(super) fn key(system: &SystemId, owner: OwnerKey) -> StateKey {
    let mut key = owner_wake_key(system, owner);
    // Zero-length system IDs are forbidden for wake flags. This discriminator
    // reserves a separate address space without changing any existing key.
    key.bytes.splice(..0, [0, 0]);
    key
}
pub(super) fn decode_key(key: &StateKey) -> io::Result<Address> {
    if !is_key(key) {
        return Err(invalid("invalid owner mailbox key"));
    }
    let key = StateKey::new(OWNER_WAKE_DOMAIN, key.bytes[2..].to_vec());
    let (system, owner) =
        decode_owner_wake_key(&key).ok_or_else(|| invalid("invalid owner mailbox address"))?;
    Ok((
        SystemId::new(system).map_err(|_| invalid("invalid mailbox system"))?,
        owner,
    ))
}
pub(super) fn encode(mailbox: &[IntentDelivery]) -> io::Result<Vec<u8>> {
    if mailbox.is_empty() {
        return Ok(Vec::new());
    }
    if mailbox.len() > MAX_MAILBOX_INTENTS {
        return Err(invalid("owner mailbox exceeds bound"));
    }
    let mut value = b"BGIM\x01".to_vec();
    value.push(mailbox.len() as u8);
    for message in mailbox {
        if message.id.revision == 0
            || message.id.ordinal as usize >= MAX_INTENTS_PER_JOB
            || message.produced_tick == 0
            || message.payload.len() > MAX_INTENT_PAYLOAD_BYTES
        {
            return Err(invalid("invalid owner intent"));
        }
        match message.id.source {
            Owner::Chunk(cell) => {
                value.push(0);
                for n in cell {
                    value.extend(n.to_le_bytes());
                }
            }
            Owner::Entity(id) => {
                value.push(1);
                value.extend(id.to_le_bytes());
            }
            Owner::Profile(id) => {
                value.push(2);
                value.extend(id.to_le_bytes());
            }
        }
        value.extend(message.id.revision.to_le_bytes());
        value.push(message.id.ordinal);
        value.extend(message.produced_tick.to_le_bytes());
        value.extend((message.payload.len() as u16).to_le_bytes());
        value.extend(&message.payload);
    }
    value.extend(crc32(&value).to_le_bytes());
    Ok(value)
}
pub(super) fn decode(value: &[u8]) -> io::Result<Mailbox> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    if value.len() < 10
        || value.len() > 10 + MAX_MAILBOX_INTENTS * (36 + MAX_INTENT_PAYLOAD_BYTES)
        || &value[..5] != b"BGIM\x01"
    {
        return Err(invalid("invalid owner mailbox envelope"));
    }
    let (body, checksum) = value.split_at(value.len() - 4);
    if crc32(body) != u32::from_le_bytes(checksum.try_into().unwrap()) {
        return Err(invalid("owner mailbox checksum mismatch"));
    }
    let count = body[5] as usize;
    if count == 0 || count > MAX_MAILBOX_INTENTS {
        return Err(invalid("invalid owner mailbox count"));
    }
    let mut reader = Reader(&body[6..]);
    let mut mailbox = Vec::with_capacity(count);
    let mut ids = BTreeSet::new();
    for _ in 0..count {
        let source = match reader.array::<1>()?[0] {
            0 => Owner::Chunk([
                i32::from_le_bytes(reader.array()?),
                i32::from_le_bytes(reader.array()?),
                i32::from_le_bytes(reader.array()?),
            ]),
            1 => Owner::Entity(u64::from_le_bytes(reader.array()?)),
            2 => Owner::Profile(u128::from_le_bytes(reader.array()?)),
            _ => return Err(invalid("invalid owner intent source")),
        };
        let id = IntentId {
            source,
            revision: u64::from_le_bytes(reader.array()?),
            ordinal: reader.array::<1>()?[0],
        };
        let produced_tick = u64::from_le_bytes(reader.array()?);
        let length = u16::from_le_bytes(reader.array()?) as usize;
        if length > MAX_INTENT_PAYLOAD_BYTES
            || id.revision == 0
            || id.ordinal as usize >= MAX_INTENTS_PER_JOB
            || produced_tick == 0
            || !ids.insert(id)
        {
            return Err(invalid("invalid or duplicated owner intent"));
        }
        let payload = reader.take(length)?.to_vec();
        if mailbox.last().is_some_and(|previous: &IntentDelivery| {
            (previous.produced_tick, previous.id) >= (produced_tick, id)
        }) {
            return Err(invalid("unordered owner mailbox"));
        }
        mailbox.push(IntentDelivery {
            id,
            produced_tick,
            payload,
        });
    }
    if !reader.0.is_empty() {
        return Err(invalid("trailing owner mailbox bytes"));
    }
    Ok(mailbox)
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if self.0.len() < n {
            return Err(invalid("truncated owner mailbox"));
        }
        let (result, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(result)
    }
    fn array<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }
}

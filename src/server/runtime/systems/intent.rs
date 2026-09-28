//! Durable same-system mailboxes, distinct from advisory effects and wake flags.
//! Mailboxes use a tagged key in the existing owner-wake WAL domain and its
//! base/tail recovery path. There is one reusable key per destination,
//! not one immortal journal tombstone per message. No side journal/checkpoint.
use crate::server::journal::{Change, StateKey};
use crate::server::parallel::{OwnerData, OwnerKey};
use crate::server::registry::SystemId;
use crate::server::runtime::owner_durable::DurableOwnerStore;
use bloxgloom_host_api::system::{IntentDelivery, MAX_INTENTS_PER_JOB};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};

mod codec;
pub(in crate::server) use codec::is_key;
use codec::{decode, decode_key, encode, key};

pub(in crate::server) const MAX_PENDING_INTENTS: usize = 2048;
pub(in crate::server) const MAX_MAILBOX_INTENTS: usize = 64;
pub(in crate::server) const MAX_WAVE_INTENTS: usize = 128;
type Address = (SystemId, OwnerKey);
type Mailbox = Vec<IntentDelivery>;

/// Assign identities only after the complete worker wave passed revision
/// validation. No callback chooses an ID or targets a foreign system.
pub(super) fn collect(
    owners: &DurableOwnerStore,
    system: &SystemId,
    patches: &[crate::server::parallel::OwnerPatch],
    tick: u64,
) -> io::Result<Vec<(OwnerKey, IntentDelivery)>> {
    let mut outgoing = Vec::new();
    for patch in patches {
        for (ordinal, request) in super::OwnerEffectPatch::intents(patch).iter().enumerate() {
            if !owners.accepts_intents(system)
                || request.payload.len() > bloxgloom_host_api::system::MAX_INTENT_PAYLOAD_BYTES
            {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "invalid owner intent output",
                ));
            }
            if ordinal >= MAX_INTENTS_PER_JOB || outgoing.len() >= MAX_WAVE_INTENTS {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "owner intent wave exceeds bound",
                ));
            }
            let destination = internal_owner(request.destination);
            if !owners.accepts_owner(system, destination) {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "intent destination has the wrong owner partition",
                ));
            }
            let revision = owners
                .revision(system, patch.owner())
                .and_then(|n| n.checked_add(1))
                .ok_or_else(|| io::Error::other("owner intent revision exhausted"))?;
            outgoing.push((
                destination,
                IntentDelivery {
                    id: bloxgloom_host_api::system::IntentId {
                        source: public_owner(patch.owner()),
                        revision,
                        ordinal: ordinal as u8,
                    },
                    produced_tick: tick,
                    payload: request.payload.clone(),
                },
            ));
        }
    }
    Ok(outgoing)
}

/// Only immutable worker input is wrapped; the authoritative value and codec
/// remain the registered owner's normal bytes.
pub(in crate::server) struct JobInput {
    pub value: OwnerData,
    pub inbox: Vec<IntentDelivery>,
}

#[derive(Debug, Default)]
pub(in crate::server) struct PreparedIntents {
    changes: Vec<Change>,
    added: usize,
}
impl PreparedIntents {
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }
}

#[derive(Default)]
pub(in crate::server) struct IntentStore {
    pending: BTreeMap<Address, Mailbox>,
    count: usize,
    staged: BTreeSet<Address>,
    staged_added: usize,
    /// Inspection, not authoritative work order. Rotates even when terrain or
    /// admission is unavailable. Restart resets to the first pending address;
    /// no message/deadline is consumed by inspection and every address is
    /// revisited within one bounded pass in a running process.
    cursor: BTreeMap<SystemId, OwnerKey>,
}
impl IntentStore {
    pub fn recover_value(&mut self, key: &StateKey, value: &[u8]) -> io::Result<()> {
        let address = decode_key(key)?;
        let mailbox = decode(value)?;
        if self.count + mailbox.len() > MAX_PENDING_INTENTS {
            return Err(invalid("recovered owner intent capacity exceeded"));
        }
        self.count += mailbox.len();
        if !mailbox.is_empty() {
            self.pending.insert(address, mailbox);
        }
        Ok(())
    }

    pub fn validate_owners(&self, owners: &DurableOwnerStore) -> io::Result<()> {
        let mut identities = BTreeSet::new();
        for ((system, owner), mailbox) in &self.pending {
            if !owners.accepts_intents(system) || owners.revision(system, *owner).is_none() {
                return Err(invalid("intent destination has no durable owner"));
            }
            for message in mailbox {
                if !identities.insert((system, message.id)) {
                    return Err(invalid(
                        "owner intent identity appears in multiple mailboxes",
                    ));
                }
                if owners
                    .revision(system, internal_owner(message.id.source))
                    .is_none_or(|revision| revision < message.id.revision)
                {
                    return Err(invalid("intent producer revision is not committed"));
                }
            }
        }
        Ok(())
    }

    /// Inspect at most one mailbox. Unavailable low destinations cannot hide
    /// higher ones indefinitely. Only previously committed ticks are eligible,
    /// including after restart (the server recovers its clock from the WAL).
    pub fn next_destination(&mut self, system: &SystemId, tick: u64) -> Option<OwnerKey> {
        let first = OwnerKey::Chunk(crate::world::ChunkKey {
            x: i32::MIN,
            y: i32::MIN,
            z: i32::MIN,
        });
        let start = self.cursor.get(system).copied();
        let bound = start.map_or(
            std::ops::Bound::Included((system.clone(), first)),
            |owner| std::ops::Bound::Excluded((system.clone(), owner)),
        );
        let candidate = self
            .pending
            .range((bound, std::ops::Bound::Unbounded))
            .next()
            .filter(|((id, _), _)| id == system)
            .or_else(|| {
                self.pending
                    .range((system.clone(), first)..)
                    .next()
                    .filter(|((id, _), _)| id == system)
            });
        let ((_, owner), mailbox) = candidate?;
        let owner = *owner;
        self.cursor.insert(system.clone(), owner);
        mailbox
            .first()
            .filter(|message| message.produced_tick < tick)
            .map(|_| owner)
    }

    pub fn capture(
        &self,
        system: &SystemId,
        owner: OwnerKey,
        tick: u64,
        remaining: usize,
    ) -> Mailbox {
        self.pending
            .get(&(system.clone(), owner))
            .into_iter()
            .flatten()
            .take_while(|message| message.produced_tick < tick)
            .take(MAX_INTENTS_PER_JOB.min(remaining))
            .cloned()
            .collect()
    }

    pub fn prepare(
        &mut self,
        system: &SystemId,
        received: &[(OwnerKey, Mailbox)],
        outgoing: &[(OwnerKey, IntentDelivery)],
    ) -> io::Result<PreparedIntents> {
        if outgoing.len() > MAX_WAVE_INTENTS
            || received.len() > MAX_WAVE_INTENTS
            || received.iter().map(|(_, inbox)| inbox.len()).sum::<usize>() > MAX_WAVE_INTENTS
        {
            return Err(invalid("owner intent wave exceeds bound"));
        }
        let mut replacements = BTreeMap::<OwnerKey, Mailbox>::new();
        for owner in received
            .iter()
            .map(|(owner, _)| owner)
            .chain(outgoing.iter().map(|(owner, _)| owner))
        {
            let address = (system.clone(), *owner);
            if self.staged.contains(&address) {
                return Err(blocked("owner mailbox is in flight"));
            }
            replacements
                .entry(*owner)
                .or_insert_with(|| self.pending.get(&address).cloned().unwrap_or_default());
        }
        for (owner, inbox) in received {
            let mailbox = replacements.get_mut(owner).expect("captured mailbox");
            for message in inbox {
                let Some(index) = mailbox.iter().position(|entry| entry == message) else {
                    return Err(blocked("owner inbox changed before acknowledgement"));
                };
                mailbox.remove(index);
            }
        }
        for (owner, message) in outgoing {
            let mailbox = replacements.get_mut(owner).expect("output mailbox");
            if let Some(existing) = mailbox.iter().find(|entry| entry.id == message.id) {
                if existing != message {
                    return Err(invalid("owner intent identity collision"));
                }
                continue;
            }
            if mailbox.len() == MAX_MAILBOX_INTENTS {
                return Err(blocked("owner destination mailbox full"));
            }
            mailbox.push(message.clone());
        }
        let mut prepared = PreparedIntents::default();
        let mut removed = 0;
        for (owner, mut mailbox) in replacements {
            mailbox.sort_by_key(|message| (message.produced_tick, message.id));
            let address = (system.clone(), owner);
            let before = self.pending.get(&address).map_or(&[][..], Vec::as_slice);
            if before == mailbox {
                continue;
            }
            prepared.added += mailbox.len().saturating_sub(before.len());
            removed += before.len().saturating_sub(mailbox.len());
            prepared.changes.push(Change::new(
                key(system, owner),
                encode(before)?,
                encode(&mailbox)?,
            ));
        }
        // Acknowledgement can finance forwarding in this *same* atomic record.
        // Do not borrow capacity from another unreceipted transaction's clears.
        prepared.added = prepared.added.saturating_sub(removed);
        if self.count + self.staged_added + prepared.added > MAX_PENDING_INTENTS {
            return Err(blocked("owner intent queue full"));
        }
        for change in &prepared.changes {
            self.staged.insert(decode_key(&change.key)?);
        }
        self.staged_added += prepared.added;
        Ok(prepared)
    }

    pub fn cancel(&mut self, prepared: PreparedIntents) {
        self.release(&prepared);
    }
    fn release(&mut self, prepared: &PreparedIntents) {
        self.staged_added -= prepared.added;
        for change in &prepared.changes {
            self.staged
                .remove(&decode_key(&change.key).expect("prepared mailbox key"));
        }
    }
    pub fn commit(&mut self, prepared: PreparedIntents) -> io::Result<()> {
        self.apply_replayed(&prepared.changes)?;
        self.release(&prepared);
        Ok(())
    }
    pub fn apply_replayed(&mut self, changes: &[Change]) -> io::Result<()> {
        let mut replacements = Vec::new();
        let mut count = self.count;
        for change in changes.iter().filter(|change| is_key(&change.key)) {
            let address = decode_key(&change.key)?;
            let before = self.pending.get(&address).map_or(&[][..], Vec::as_slice);
            if encode(before)? != change.before {
                return Err(invalid("owner mailbox preimage mismatch"));
            }
            let after = decode(&change.after)?;
            count = count - before.len() + after.len();
            replacements.push((address, after));
        }
        if count > MAX_PENDING_INTENTS {
            return Err(invalid("committed owner intent capacity exceeded"));
        }
        for (address, after) in replacements {
            if after.is_empty() {
                self.pending.remove(&address);
            } else {
                self.pending.insert(address, after);
            }
        }
        self.count = count;
        Ok(())
    }
}

pub(in crate::server) fn internal_owner(owner: bloxgloom_host_api::system::Owner) -> OwnerKey {
    use bloxgloom_host_api::system::Owner;
    match owner {
        Owner::Chunk([x, y, z]) => OwnerKey::Chunk(crate::world::ChunkKey { x, y, z }),
        Owner::Entity(id) => OwnerKey::Entity(id),
        Owner::Profile(id) => OwnerKey::Profile(id),
    }
}
pub(in crate::server) fn public_owner(owner: OwnerKey) -> bloxgloom_host_api::system::Owner {
    use bloxgloom_host_api::system::Owner;
    match owner {
        OwnerKey::Chunk(key) => Owner::Chunk([key.x, key.y, key.z]),
        OwnerKey::Entity(id) => Owner::Entity(id),
        OwnerKey::Profile(id) => Owner::Profile(id),
    }
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
fn blocked(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::WouldBlock, message)
}

#[cfg(test)]
mod tests;

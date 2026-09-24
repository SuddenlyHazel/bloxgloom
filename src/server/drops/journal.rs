//! WAL owner codecs and startup recovery for drops.
use std::collections::BTreeMap;
use std::io;
use std::time::{Duration, Instant};

use crate::inventory::{ComponentPayload, MAX_COMPONENT_BYTES, STACK_LIMIT};
use crate::items::ItemId;
use crate::protocol::DroppedItem;
use crate::server::journal::{CompactedDrop, DropCompaction, StateKey};
use std::sync::Arc;

use super::{Drops, Entry, LIFETIME, invalid, unix_ms};

impl Drops {
    /// Startup applies the latest journal values over the validated BGDP
    /// snapshot. Existing checkpoint positions remain authoritative.
    pub(in crate::server) fn apply_recovered_journal(
        &mut self,
        values: &BTreeMap<StateKey, Vec<u8>>,
        drop_owner_set_closed: bool,
    ) -> io::Result<bool> {
        self.validate_recovered_journal(values, drop_owner_set_closed)?;
        let mut owners = BTreeMap::<u64, Vec<u8>>::new();
        let mut positions = BTreeMap::<u64, [f32; 3]>::new();
        let mut allocator = None;
        for (key, value) in values {
            match key.domain.as_str() {
                "bloxgloom:drop_owner" => {
                    let id = decode_drop_id(&key.bytes)?;
                    owners.insert(id, value.clone());
                }
                "bloxgloom:drop_position" => {
                    let id = decode_drop_id(&key.bytes)?;
                    if !value.is_empty() {
                        positions.insert(id, decode_position(value)?);
                    }
                }
                "bloxgloom:drop_allocator" => {
                    if value.len() != 8 {
                        return Err(invalid("invalid journaled drop allocator"));
                    }
                    allocator = Some(u64::from_le_bytes(value[..8].try_into().unwrap()));
                }
                "bloxgloom:chunk_snapshot"
                | "bloxgloom:inventory"
                | "bloxgloom:action_receipt"
                | "bloxgloom:action_ledger"
                | "bloxgloom:drops_snapshot" => {}
                domain => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("unsupported journal key domain: {domain}"),
                    ));
                }
            }
        }

        let mut touched = false;
        for (id, owner) in owners {
            if owner.is_empty() {
                if self.entries.remove(&id).is_some() {
                    self.remove_entry_indexes(id);
                    touched = true;
                }
                self.active.remove(&id);
                continue;
            }
            let current = self.entries.get(&id).map(encode_owner).unwrap_or_default();
            if current == owner {
                continue;
            }
            let (item, count, born, delay, components) =
                decode_owner_with_catalog(id, &owner, &self.catalog)?;
            if let Some(entry) = self.entries.get_mut(&id) {
                entry.item.item = item;
                entry.item.count = count;
                entry.components = components;
                entry.created_unix_ms = born;
                let age = Duration::from_millis(unix_ms().saturating_sub(born));
                entry.age_at_load = age;
                entry.age_since = Instant::now();
                entry.pickup_delay = Duration::from_millis(u64::from(delay));
                self.expiry.insert(id, age, entry.age_since);
                // Recovery overlays ownership, not motion state. Preserve a
                // checkpointed sleeping drop instead of waking it due to a
                // count-only change.
                if age >= LIFETIME {
                    self.active.remove(&id);
                }
                touched = true;
            } else {
                let position = positions
                    .get(&id)
                    .copied()
                    .ok_or_else(|| invalid("journaled drop is missing its spawn position"))?;
                let age = Duration::from_millis(unix_ms().saturating_sub(born));
                let age_since = Instant::now();
                self.entries.insert(
                    id,
                    Entry {
                        item: DroppedItem {
                            id,
                            item,
                            count,
                            position,
                            age_ms: 0,
                        },
                        components,
                        vertical_speed: 0.0,
                        age_at_load: age,
                        age_since,
                        created_unix_ms: born,
                        pickup_delay: Duration::from_millis(u64::from(delay)),
                    },
                );
                self.spatial.insert(id, position);
                self.expiry.insert(id, age, age_since);
                if age < LIFETIME {
                    self.active.insert(id);
                }
                touched = true;
            }
        }
        if let Some(value) = allocator {
            let previous = self.next_id;
            self.next_id = self.next_id.max(value);
            touched |= self.next_id != previous;
        }
        if touched {
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(touched)
    }

    pub(in crate::server) fn validate_recovered_journal(
        &self,
        values: &BTreeMap<StateKey, Vec<u8>>,
        drop_owner_set_closed: bool,
    ) -> io::Result<()> {
        let mut owners = BTreeMap::<u64, &[u8]>::new();
        let mut positions = BTreeMap::<u64, [f32; 3]>::new();
        let mut allocator = None;
        for (key, value) in values {
            match key.domain.as_str() {
                "bloxgloom:drop_owner" => {
                    let id = decode_drop_id(&key.bytes)?;
                    if id == 0 {
                        return Err(invalid("invalid journaled drop key"));
                    }
                    if !value.is_empty() {
                        decode_owner_with_catalog(id, value, &self.catalog)?;
                    }
                    owners.insert(id, value);
                }
                "bloxgloom:drop_position" => {
                    let id = decode_drop_id(&key.bytes)?;
                    if !value.is_empty() {
                        positions.insert(id, decode_position(value)?);
                    }
                }
                "bloxgloom:drop_allocator" => {
                    if value.len() != 8 {
                        return Err(invalid("invalid journaled drop allocator"));
                    }
                    let next_id = u64::from_le_bytes(value[..8].try_into().unwrap());
                    if next_id == 0 {
                        return Err(invalid("invalid journaled drop allocator"));
                    }
                    allocator = Some(next_id);
                }
                "bloxgloom:chunk_snapshot"
                | "bloxgloom:inventory"
                | "bloxgloom:action_receipt"
                | "bloxgloom:action_ledger"
                | "bloxgloom:drops_snapshot" => {}
                domain => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("unsupported journal key domain: {domain}"),
                    ));
                }
            }
        }
        for (&id, owner) in &owners {
            if !owner.is_empty() && !self.entries.contains_key(&id) && !positions.contains_key(&id)
            {
                return Err(invalid("journaled drop is missing its spawn position"));
            }
        }
        if drop_owner_set_closed {
            for id in self.entries.keys() {
                if !owners.contains_key(id) {
                    return Err(invalid(
                        "drops snapshot contains an ID outside the closed journal owner set",
                    ));
                }
            }
        }
        if let Some(next_id) = allocator {
            let maximum_live = owners
                .iter()
                .filter_map(|(&id, owner)| (!owner.is_empty()).then_some(id))
                .max()
                .unwrap_or(0);
            if next_id <= maximum_live {
                return Err(invalid("journaled drop allocator is behind live IDs"));
            }
        }
        Ok(())
    }

    /// Materializes the authoritative live drop set for a rotation cut. This
    /// includes legacy BGDP-only drops and their current motion positions,
    /// allowing the new generation to discard all historical owner tombstones
    /// and stale spawn positions without ever reusing drop IDs.
    pub(in crate::server) fn rotation_compaction(&self) -> DropCompaction {
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by_key(|entry| entry.item.id);
        DropCompaction {
            drops: entries
                .into_iter()
                .map(|entry| CompactedDrop {
                    id: entry.item.id,
                    owner: encode_owner(entry),
                    position: entry.item.position,
                })
                .collect(),
            next_id: self.next_id,
        }
    }
}

pub(super) fn encode_owner(entry: &Entry) -> Vec<u8> {
    let component_bytes = entry
        .components
        .as_ref()
        .map_or(0, |payload| payload.bytes.len());
    let mut bytes = Vec::with_capacity(21 + component_bytes);
    bytes.push(3);
    bytes.extend(entry.item.item.get().to_le_bytes());
    bytes.extend(entry.item.count.to_le_bytes());
    bytes.extend(entry.created_unix_ms.to_le_bytes());
    bytes.extend((entry.pickup_delay.as_millis().min(u16::MAX as u128) as u16).to_le_bytes());
    if let Some(payload) = &entry.components {
        bytes.extend(payload.version.to_le_bytes());
        bytes.extend((payload.bytes.len() as u16).to_le_bytes());
        bytes.extend(&payload.bytes);
    } else {
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
    }
    bytes
}

pub(in crate::server) fn decode_owner(
    id: u64,
    bytes: &[u8],
) -> io::Result<(ItemId, u16, u64, u16, Option<Arc<ComponentPayload>>)> {
    decode_owner_with_catalog(id, bytes, crate::content::catalog())
}

pub(in crate::server) fn decode_owner_with_catalog(
    id: u64,
    bytes: &[u8],
    catalog: &crate::content::Catalog,
) -> io::Result<(ItemId, u16, u64, u16, Option<Arc<ComponentPayload>>)> {
    if bytes.len() < 21 || bytes[0] != 3 {
        return Err(invalid("invalid journaled drop ownership"));
    }
    let item = ItemId::new(u32::from_le_bytes(bytes[1..5].try_into().unwrap()));
    let count = u16::from_le_bytes(bytes[5..7].try_into().unwrap());
    let born = u64::from_le_bytes(bytes[7..15].try_into().unwrap());
    let delay = u16::from_le_bytes(bytes[15..17].try_into().unwrap());
    let component_version = u16::from_le_bytes(bytes[17..19].try_into().unwrap());
    let component_len = u16::from_le_bytes(bytes[19..21].try_into().unwrap()) as usize;
    if id == 0 || catalog.item(item).is_none() || !(1..=STACK_LIMIT).contains(&count) {
        return Err(invalid("invalid journaled drop ownership"));
    }
    if bytes.len() != 21 + component_len || component_len > MAX_COMPONENT_BYTES {
        return Err(invalid("invalid journaled drop component length"));
    }
    let components = if component_len == 0 {
        if component_version != 0 {
            return Err(invalid("invalid empty drop components"));
        }
        None
    } else {
        Some(Arc::new(
            ComponentPayload::new(component_version, bytes[21..].to_vec())
                .ok_or_else(|| invalid("invalid drop components"))?,
        ))
    };
    Ok((item, count, born, delay, components))
}

fn decode_drop_id(bytes: &[u8]) -> io::Result<u64> {
    if bytes.len() != 8 {
        return Err(invalid("invalid journaled drop key"));
    }
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

fn decode_position(bytes: &[u8]) -> io::Result<[f32; 3]> {
    if bytes.len() != 12 {
        return Err(invalid("invalid journaled drop position"));
    }
    let position = [
        f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    ];
    if position.iter().any(|coordinate| !coordinate.is_finite()) {
        return Err(invalid("invalid journaled drop position"));
    }
    Ok(position)
}

//! BGCM v2 content assignments. Layout: magic[4], version u16, entry count
//! u32, then sorted entries `(kind u8, id u32, key_len u16, key bytes,
//! schema_fingerprint u64)`, followed by FNV-1a checksum u32. All integers
//! are little-endian. IDs in removed entries stay reserved.

use std::collections::{BTreeMap, HashSet};
use std::io;

use super::{
    BlockStateId, BlockTypeId, Catalog, EntityTypeId, ItemId, MAX_ASSIGNED_ID, valid_key,
    valid_property_token,
};

pub const MAX_MANIFEST_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = MAX_ASSIGNED_ID as usize * 4;
const MAGIC: &[u8; 4] = b"BGCM";
const VERSION: u16 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentEntry {
    pub kind: u8,
    pub id: u32,
    pub key: String,
    pub schema_fingerprint: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentManifest {
    pub entries: Vec<ContentEntry>,
}

impl ContentManifest {
    pub fn from_catalog(catalog: &Catalog) -> Self {
        let mut entries = catalog
            .identities()
            .into_iter()
            .map(|(kind, id, key, schema_fingerprint)| ContentEntry {
                kind,
                id,
                key: key.to_owned(),
                schema_fingerprint,
            })
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|entry| (entry.kind, entry.id));
        Self { entries }
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::with_capacity(10 + self.entries.len() * 32 + 4);
        out.extend(MAGIC);
        out.extend(VERSION.to_le_bytes());
        out.extend((self.entries.len() as u32).to_le_bytes());
        for entry in &self.entries {
            out.push(entry.kind);
            out.extend(entry.id.to_le_bytes());
            out.extend((entry.key.len() as u16).to_le_bytes());
            out.extend(entry.key.as_bytes());
            out.extend(entry.schema_fingerprint.to_le_bytes());
            if out.len() + 4 > MAX_MANIFEST_BYTES {
                return Err(invalid("content manifest too large"));
            }
        }
        out.extend(checksum(&out).to_le_bytes());
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 14 || bytes.len() > MAX_MANIFEST_BYTES || &bytes[..4] != MAGIC {
            return Err(invalid("invalid content manifest header"));
        }
        if u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != VERSION {
            return Err(invalid("unsupported content manifest version"));
        }
        let count = u32::from_le_bytes(bytes[6..10].try_into().unwrap()) as usize;
        let body_end = bytes.len() - 4;
        if count > MAX_ENTRIES || count > (body_end - 10) / 15 {
            return Err(invalid("invalid content manifest count"));
        }
        let stored = u32::from_le_bytes(bytes[body_end..].try_into().unwrap());
        if stored != checksum(&bytes[..body_end]) {
            return Err(invalid("content manifest checksum mismatch"));
        }
        let mut entries = Vec::with_capacity(count.min(262_144));
        let mut offset = 10usize;
        for _ in 0..count {
            if offset + 7 > body_end {
                return Err(invalid("truncated content manifest"));
            }
            let kind = bytes[offset];
            let id = u32::from_le_bytes(bytes[offset + 1..offset + 5].try_into().unwrap());
            let key_len =
                u16::from_le_bytes(bytes[offset + 5..offset + 7].try_into().unwrap()) as usize;
            offset += 7;
            let end = offset
                .checked_add(key_len + 8)
                .ok_or_else(|| invalid("invalid content key length"))?;
            if end > body_end {
                return Err(invalid("truncated content manifest"));
            }
            let key = std::str::from_utf8(&bytes[offset..offset + key_len])
                .map_err(|_| invalid("invalid content key"))?
                .to_owned();
            offset += key_len;
            let schema_fingerprint =
                u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
            offset += 8;
            entries.push(ContentEntry {
                kind,
                id,
                key,
                schema_fingerprint,
            });
        }
        if offset != body_end {
            return Err(invalid("trailing content manifest bytes"));
        }
        let manifest = Self { entries };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Resolves candidate definitions to persistent world assignments before
    /// opening chunk data. New keys receive IDs above that kind's high-water
    /// mark in canonical key order; absent definitions keep their old IDs
    /// reserved in `self`.
    pub fn resolve_world_catalog(&mut self, local: &Catalog) -> io::Result<(Catalog, bool)> {
        self.validate()?;
        local
            .validate()
            .map_err(|_| invalid("incomplete local content catalog"))?;
        let current = Self::from_catalog(local);
        let mut saved_by_key = self
            .entries
            .iter()
            .map(|entry| {
                (
                    (entry.kind, entry.key.clone()),
                    (entry.id, entry.schema_fingerprint),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut high_water = BTreeMap::<u8, u32>::new();
        for entry in &self.entries {
            high_water
                .entry(entry.kind)
                .and_modify(|id| *id = (*id).max(entry.id))
                .or_insert(entry.id);
        }
        let mut candidates = current.entries;
        candidates.sort_unstable_by(|a, b| (a.kind, a.key.as_str()).cmp(&(b.kind, b.key.as_str())));
        let mut active = Vec::with_capacity(candidates.len());
        let mut changed = false;
        for mut entry in candidates {
            if let Some(&(id, fingerprint)) = saved_by_key.get(&(entry.kind, entry.key.clone())) {
                if fingerprint != entry.schema_fingerprint {
                    return Err(invalid(if entry.key == "bloxgloom:player" {
                        "incompatible player character catalog; use a new world directory (existing save left unchanged)"
                    } else {
                        "content schema changed without migration"
                    }));
                }
                entry.id = id;
            } else {
                let next = high_water
                    .get(&entry.kind)
                    .map_or(0, |id| id.saturating_add(1));
                if next >= MAX_ASSIGNED_ID
                    || (entry.kind == b'I' && next == 0)
                    || (entry.kind == b'E' && next == 0)
                {
                    return Err(invalid("content ID high-water exhausted"));
                }
                entry.id = next;
                high_water.insert(entry.kind, next);
                saved_by_key.insert(
                    (entry.kind, entry.key.clone()),
                    (entry.id, entry.schema_fingerprint),
                );
                self.entries.push(entry.clone());
                changed = true;
            }
            active.push(entry);
        }
        active.sort_unstable_by_key(|entry| (entry.kind, entry.id));
        let catalog = Self { entries: active }.resolve_catalog(local)?;
        if changed {
            self.entries
                .sort_unstable_by_key(|entry| (entry.kind, entry.id));
            self.validate()?;
        }
        Ok((catalog, changed))
    }

    /// Resolves local definitions to the server's numeric assignments. The
    /// handshake sends the active catalog manifest; a save's reserved/tombstone
    /// entries are not part of this per-connection manifest.
    pub fn resolve_catalog(&self, local: &Catalog) -> io::Result<Catalog> {
        self.validate()?;
        local
            .validate()
            .map_err(|_| invalid("incomplete local content catalog"))?;
        let local_manifest = Self::from_catalog(local);
        let local_by_key = local_manifest
            .entries
            .iter()
            .map(|entry| ((entry.kind, entry.key.as_str()), entry))
            .collect::<BTreeMap<_, _>>();
        if self.entries.len() != local_by_key.len() {
            return Err(invalid("client and server content definitions differ"));
        }
        let mut assigned = BTreeMap::new();
        for entry in &self.entries {
            let Some(local_entry) = local_by_key.get(&(entry.kind, entry.key.as_str())) else {
                return Err(invalid("missing client content definition"));
            };
            if local_entry.schema_fingerprint != entry.schema_fingerprint {
                return Err(invalid("client content schema or material differs"));
            }
            assigned.insert((entry.kind, entry.key.as_str()), entry.id);
        }

        let mut resolved = Catalog::new();
        resolved.player_rules = local.player_rules();
        resolved.player_selection = local.player_selection.clone();
        resolved.player_appearance = local.player_appearance.clone();
        for entry in self.entries.iter().filter(|entry| entry.kind == b'Q') {
            if let Some(registration) = local
                .player_lifecycles
                .values()
                .find(|r| r.key == entry.key)
            {
                resolved
                    .player_lifecycles
                    .insert(entry.id, registration.clone());
            }
        }
        for entry in self.entries.iter().filter(|entry| entry.kind == b'O') {
            if let Some(observer) = local
                .gameplay_observers
                .values()
                .find(|o| o.key == entry.key)
            {
                resolved
                    .gameplay_observers
                    .insert(entry.id, observer.clone());
            }
        }
        resolved.gameplay_dispatch = local.gameplay_dispatch.clone();
        resolved.client_metadata.entities = local.client_metadata.entities.clone();
        // The exact key/schema comparison above already verified these hashes.
        // Remap inert identities without inventing executable client callbacks.
        for entry in self
            .entries
            .iter()
            .filter(|entry| matches!(entry.kind, b'G' | b'Y' | b'Q' | b'O'))
        {
            if local
                .client_metadata
                .identities
                .iter()
                .any(|((kind, _), (key, _))| *kind == entry.kind && *key == entry.key)
            {
                resolved.client_metadata.identities.insert(
                    (entry.kind, entry.id),
                    (entry.key.clone(), entry.schema_fingerprint),
                );
            }
        }
        for entry in self.entries.iter().filter(|entry| entry.kind == b'G') {
            if resolved
                .client_metadata
                .identities
                .contains_key(&(entry.kind, entry.id))
            {
                continue;
            }
            let handler = local
                .gameplay_handlers
                .values()
                .find(|h| h.key == entry.key)
                .ok_or_else(|| invalid("missing gameplay handler"))?;
            resolved.gameplay_handlers.insert(entry.id, handler.clone());
        }
        for entry in self.entries.iter().filter(|entry| entry.kind == b'Y') {
            if resolved
                .client_metadata
                .identities
                .contains_key(&(entry.kind, entry.id))
            {
                continue;
            }
            let system = local
                .owner_systems()
                .find(|s| s.key == entry.key)
                .ok_or_else(|| invalid("missing owner system"))?;
            resolved.owner_systems.insert(entry.id, system.clone());
        }
        resolved.narrow_plants = local.narrow_plants.clone();
        resolved.block_acoustics = local.block_acoustics.clone();
        resolved.item_components = local.item_components.clone();
        resolved.drop_sizes = local.drop_sizes.clone();
        resolved.drop_animations = local.drop_animations.clone();
        resolved.drop_policies = local.drop_policies.clone();
        resolved.max_drop_pickup_range = local.max_drop_pickup_range;
        resolved.item_icons = local.item_icons.clone();
        resolved.composition = local.composition.clone();
        for entry in self
            .entries
            .iter()
            .filter(|e| matches!(e.kind, b'P' | b'T' | b'U'))
        {
            if !resolved.composition.remap(entry.kind, &entry.key, entry.id) {
                return Err(invalid("missing composition definition"));
            }
        }
        resolved.storage_lifecycles = local.storage_lifecycles.clone();
        resolved.textures = local.textures.clone();
        resolved.texture_fingerprints = local.texture_fingerprints.clone();
        resolved.texture_keys = local.texture_keys.clone();
        for entry in self.entries.iter().filter(|entry| entry.kind == b'B') {
            let source = local_by_key[&(b'B', entry.key.as_str())];
            let mut block = local.block_type(BlockTypeId(source.id)).unwrap().clone();
            block.id = BlockTypeId(entry.id);
            resolved
                .register_block(block)
                .map_err(|_| invalid("invalid mapped block type"))?;
        }
        for entry in self.entries.iter().filter(|entry| entry.kind == b'S') {
            let source = local_by_key[&(b'S', entry.key.as_str())];
            let state = local.state(BlockStateId(source.id)).unwrap();
            let block_key = local.block_type(state.block_type).unwrap().key.as_ref();
            let block_id = *assigned
                .get(&(b'B', block_key))
                .ok_or_else(|| invalid("missing mapped block type"))?;
            resolved
                .register_state_with_emission(
                    BlockStateId(entry.id),
                    BlockTypeId(block_id),
                    state.properties.clone(),
                    Some(state.textures),
                    Some(state.emission),
                )
                .map_err(|_| invalid("invalid mapped block state"))?;
        }
        for entry in self.entries.iter().filter(|entry| entry.kind == b'I') {
            let source = local_by_key[&(b'I', entry.key.as_str())];
            let mut item = local.item(ItemId(source.id)).unwrap().clone();
            item.id = ItemId(entry.id);
            if let Some(state) = item.placeable {
                let key = local.state(state).unwrap().key.as_str();
                item.placeable = Some(BlockStateId(
                    *assigned
                        .get(&(b'S', key))
                        .ok_or_else(|| invalid("missing mapped placement state"))?,
                ));
            }
            resolved
                .register_item(item)
                .map_err(|_| invalid("invalid mapped item"))?;
        }
        for entry in self.entries.iter().filter(|entry| entry.kind == b'E') {
            let source = local_by_key[&(b'E', entry.key.as_str())];
            let mut entity = local.entity_type(EntityTypeId(source.id)).unwrap().clone();
            entity.id = EntityTypeId(entry.id);
            resolved
                .register_entity_type(entity)
                .map_err(|_| invalid("invalid mapped entity type"))?;
        }
        for entry in self.entries.iter().filter(|entry| entry.kind == b'M') {
            let model = local
                .model_by_key(&entry.key)
                .ok_or_else(|| invalid("missing model definition"))?;
            resolved
                .bind_model(entry.id, entry.key.clone(), model.clone())
                .map_err(|_| invalid("invalid mapped model"))?;
        }
        resolved.gameplay_entities = local.gameplay_entities.clone();
        for (_, mobile) in local.mobile_entities() {
            let id = resolved
                .entity_type_id_by_key(&mobile.key)
                .ok_or_else(|| invalid("missing mobile type"))?;
            resolved
                .bind_mobile(id, mobile.clone())
                .map_err(|_| invalid("invalid mobile binding"))?;
        }
        for (_, moving) in local.moving_entities() {
            let id = resolved
                .entity_type_id_by_key(&moving.key)
                .ok_or_else(|| invalid("missing moving identity"))?;
            resolved
                .bind_moving(id, moving.clone())
                .map_err(|_| invalid("invalid moving binding"))?;
        }
        for (_, definition) in local.anchored_entities() {
            let id = resolved
                .entity_type_id_by_key(&definition.entity)
                .ok_or_else(|| invalid("missing anchored identity"))?;
            resolved
                .bind_anchored(id, definition.clone())
                .map_err(|_| invalid("invalid anchored binding"))?;
        }
        for (_, screen) in local.inventory_screens() {
            resolved
                .register_inventory_screen((**screen).clone())
                .map_err(|_| invalid("invalid mapped inventory screen"))?;
        }
        for (_, machine) in local.machines() {
            let id = resolved
                .entity_type_id_by_key(&machine.entity)
                .ok_or_else(|| invalid("missing machine identity"))?;
            resolved
                .bind_machine(id, machine.clone())
                .map_err(|_| invalid("invalid machine binding"))?;
        }
        for action in local.actions.values() {
            if resolved.action(&action.key).is_none() {
                resolved
                    .register_action((**action).clone())
                    .map_err(|_| invalid("invalid mapped action"))?;
            }
        }
        if Self::from_catalog(&resolved) != *self {
            return Err(invalid("mapped catalog does not match content manifest"));
        }
        resolved
            .validate()
            .map_err(|_| invalid("incomplete resolved content catalog"))?;
        Ok(resolved)
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.entries.len() > MAX_ENTRIES {
            return Err(invalid("too many content identities"));
        }
        let mut previous = None;
        let mut keys = HashSet::with_capacity(self.entries.len().min(262_144));
        for entry in &self.entries {
            if !matches!(
                entry.kind,
                b'B' | b'S' | b'I' | b'E' | b'P' | b'T' | b'U' | b'Y' | b'G' | b'O' | b'Q' | b'M'
            ) || entry.id >= MAX_ASSIGNED_ID
                || entry.key.is_empty()
                || entry.key.len() > if entry.kind == b'S' { 512 } else { 255 }
                || !if entry.kind == b'S' {
                    valid_state_key(&entry.key)
                } else {
                    valid_key(&entry.key)
                }
                || previous.is_some_and(|old| old >= (entry.kind, entry.id))
                || !keys.insert((entry.kind, entry.key.as_str()))
            {
                return Err(invalid("invalid or duplicate content identity"));
            }
            previous = Some((entry.kind, entry.id));
        }
        Ok(())
    }
}

fn valid_state_key(key: &str) -> bool {
    let Some((block, suffix)) = key.split_once('[') else {
        return valid_key(key);
    };
    if !valid_key(block) || !suffix.ends_with(']') {
        return false;
    }
    let properties = &suffix[..suffix.len() - 1];
    let mut previous = "";
    let mut count = 0;
    for property in properties.split(',') {
        let Some((name, value)) = property.split_once('=') else {
            return false;
        };
        if !valid_property_token(name) || !valid_property_token(value) || name <= previous {
            return false;
        }
        previous = name;
        count += 1;
    }
    count <= 8
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
#[path = "manifest/tests.rs"]
mod tests;

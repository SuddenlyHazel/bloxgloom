//! Bounded assembly of authoritative chunk/entity snapshots and WAL-order commits.
//! Nothing mutates the visible client world until every part of a group validates.

use crate::content::Catalog;
use crate::protocol::{
    EntitySnapshotPage, PublicEntity, PublicEntityChange, ServerMessage, WorldCommitPart,
    WorldSnapshotStart, server_wire_len, snapshot_checksum,
};
use crate::render::{MAX_AVATARS, VisualAvatar};
use crate::world::{Chunk, ChunkKey};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

mod avatar;
pub(super) mod kiln;
mod mossbun;
mod registry;
pub(in crate::client) use registry::{EntityClientRegistry, EntityVerb};

const MAX_PENDING_SNAPSHOTS: usize = 8;
const MAX_PENDING_COMMITS: usize = 8;
const MAX_PENDING_BYTES: usize = 8 * 1024 * 1024;
const MAX_CHUNK_ENTITY_BYTES: usize = 1024 * 1024;
const MAX_CLIENT_ENTITY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct PendingSnapshot {
    epoch: u64,
    start: Option<WorldSnapshotStart>,
    pages: BTreeMap<u16, EntitySnapshotPage>,
    bytes: usize,
}

#[derive(Default)]
struct PendingCommit {
    count: u16,
    parts: BTreeMap<u16, WorldCommitPart>,
    bytes: usize,
}

#[derive(Default)]
pub(super) struct Replicas {
    epochs: HashMap<ChunkKey, u64>,
    entity_revisions: HashMap<ChunkKey, u64>,
    entities: HashMap<ChunkKey, BTreeMap<u64, PublicEntity>>,
    avatar_ids_by_chunk: HashMap<ChunkKey, Vec<u64>>,
    avatars: HashMap<u64, VisualAvatar>,
    snapshots: HashMap<ChunkKey, PendingSnapshot>,
    commits: BTreeMap<u64, PendingCommit>,
    last_commit: u64,
}

pub(super) enum Assembly {
    Waiting,
    Installed(Vec<ChunkKey>),
    Resync(Vec<ChunkKey>),
}

impl Replicas {
    pub(super) fn kiln_at(&self, target: [i32; 3]) -> Option<&PublicEntity> {
        let key = crate::world::world_to_chunk(target[0], target[1], target[2]).0;
        self.entities.get(&key)?.values().find(|entity| {
            entity.entity_type == crate::content::KILN_ENTITY_TYPE &&
            matches!(entity.location, crate::protocol::PublicEntityLocation::Anchored { anchor, .. }
                if target[0] == anchor[0] && target[2] == anchor[2]
                && (target[1] == anchor[1] || anchor[1].checked_add(1) == Some(target[1])))
        })
    }
    pub(super) fn accept(
        &mut self,
        message: ServerMessage,
        catalog: &Catalog,
        chunks: &mut HashMap<ChunkKey, Arc<Chunk>>,
        registry: &EntityClientRegistry,
    ) -> Assembly {
        match message {
            ServerMessage::WorldSnapshotStart(start) => {
                self.snapshot_start(start, catalog, chunks, registry)
            }
            ServerMessage::EntitySnapshotPage(page) => {
                self.snapshot_page(page, catalog, chunks, registry)
            }
            ServerMessage::WorldCommitPart(part) => self.commit_part(part, chunks, registry),
            _ => unreachable!("only entity replication messages enter the assembler"),
        }
    }

    pub(super) fn retain(&mut self, mut keep: impl FnMut(ChunkKey) -> bool) {
        let evicted: Vec<_> = self
            .entities
            .keys()
            .copied()
            .filter(|key| !keep(*key))
            .collect();
        self.epochs.retain(|key, _| keep(*key));
        self.entity_revisions.retain(|key, _| keep(*key));
        self.entities.retain(|key, _| keep(*key));
        self.snapshots.retain(|key, _| keep(*key));
        self.commits
            .retain(|_, pending| pending.parts.values().all(|part| keep(part.key)));
        for key in evicted {
            if let Some(ids) = self.avatar_ids_by_chunk.remove(&key) {
                for id in ids {
                    self.avatars.remove(&id);
                }
            }
        }
    }

    pub(super) fn visual_avatars(
        &self,
        camera: glam::Vec3,
        owned: Option<u64>,
    ) -> Vec<VisualAvatar> {
        let mut avatars: Vec<_> = self
            .avatars
            .values()
            .filter(|avatar| Some(avatar.id) != owned)
            .copied()
            .collect();
        avatars.sort_unstable_by(|a, b| {
            a.position
                .distance_squared(camera)
                .total_cmp(&b.position.distance_squared(camera))
                .then_with(|| a.id.cmp(&b.id))
        });
        avatars.truncate(MAX_AVATARS);
        avatars
    }

    #[cfg(test)]
    pub(super) fn entities_in(&self, key: ChunkKey) -> Option<&BTreeMap<u64, PublicEntity>> {
        self.entities.get(&key)
    }

    fn snapshot_start(
        &mut self,
        start: WorldSnapshotStart,
        catalog: &Catalog,
        chunks: &mut HashMap<ChunkKey, Arc<Chunk>>,
        registry: &EntityClientRegistry,
    ) -> Assembly {
        let key = start.chunk.key;
        if self
            .epochs
            .get(&key)
            .is_some_and(|epoch| start.epoch <= *epoch)
        {
            return Assembly::Waiting;
        }
        if self.snapshots.len() >= MAX_PENDING_SNAPSHOTS && !self.snapshots.contains_key(&key) {
            return Assembly::Resync(vec![key]);
        }
        let incoming_len = server_wire_len(&ServerMessage::WorldSnapshotStart(start.clone()));
        let pending = self.snapshots.entry(key).or_default();
        if start.epoch < pending.epoch {
            return Assembly::Waiting;
        }
        if start.epoch > pending.epoch {
            *pending = PendingSnapshot {
                epoch: start.epoch,
                ..PendingSnapshot::default()
            };
        }
        if let Some(old) = &pending.start {
            if old.chunk.version != start.chunk.version
                || old.entity_revision != start.entity_revision
                || old.entity_page_count != start.entity_page_count
                || old.checksum != start.checksum
                || old.chunk.blocks != start.chunk.blocks
            {
                self.snapshots.remove(&key);
                return Assembly::Resync(vec![key]);
            }
            return Assembly::Waiting;
        }
        pending.bytes = pending.bytes.saturating_add(incoming_len);
        pending.start = Some(start);
        self.finish_snapshot(key, catalog, chunks, registry)
    }

    fn snapshot_page(
        &mut self,
        page: EntitySnapshotPage,
        catalog: &Catalog,
        chunks: &mut HashMap<ChunkKey, Arc<Chunk>>,
        registry: &EntityClientRegistry,
    ) -> Assembly {
        let key = page.key;
        if self
            .epochs
            .get(&key)
            .is_some_and(|epoch| page.epoch <= *epoch)
        {
            return Assembly::Waiting;
        }
        if self.snapshots.len() >= MAX_PENDING_SNAPSHOTS && !self.snapshots.contains_key(&key) {
            return Assembly::Resync(vec![key]);
        }
        let incoming_len = server_wire_len(&ServerMessage::EntitySnapshotPage(page.clone()));
        let pending = self.snapshots.entry(key).or_default();
        if page.epoch < pending.epoch {
            return Assembly::Waiting;
        }
        if page.epoch > pending.epoch {
            *pending = PendingSnapshot {
                epoch: page.epoch,
                ..PendingSnapshot::default()
            };
        }
        if let Some(old) = pending.pages.get(&page.page_index) {
            if old != &page {
                self.snapshots.remove(&key);
                return Assembly::Resync(vec![key]);
            }
            return Assembly::Waiting;
        }
        pending.bytes = pending.bytes.saturating_add(incoming_len);
        pending.pages.insert(page.page_index, page);
        self.finish_snapshot(key, catalog, chunks, registry)
    }

    fn finish_snapshot(
        &mut self,
        key: ChunkKey,
        catalog: &Catalog,
        chunks: &mut HashMap<ChunkKey, Arc<Chunk>>,
        registry: &EntityClientRegistry,
    ) -> Assembly {
        let Some(pending) = self.snapshots.get(&key) else {
            return Assembly::Waiting;
        };
        if pending.bytes > MAX_PENDING_BYTES || self.pending_bytes() > MAX_PENDING_BYTES {
            self.snapshots.remove(&key);
            return Assembly::Resync(vec![key]);
        }
        let Some(start) = &pending.start else {
            return Assembly::Waiting;
        };
        if pending.pages.len() > usize::from(start.entity_page_count) {
            self.snapshots.remove(&key);
            return Assembly::Resync(vec![key]);
        }
        if pending.pages.len() != usize::from(start.entity_page_count) {
            return Assembly::Waiting;
        }
        let mut ordered = Vec::with_capacity(pending.pages.len());
        for index in 0..start.entity_page_count {
            let Some(page) = pending.pages.get(&index) else {
                self.snapshots.remove(&key);
                return Assembly::Resync(vec![key]);
            };
            if page.epoch != start.epoch
                || page.entity_revision != start.entity_revision
                || page.page_count != start.entity_page_count
                || page.checksum != start.checksum
            {
                self.snapshots.remove(&key);
                return Assembly::Resync(vec![key]);
            }
            ordered.push(page.entities.clone());
        }
        let valid_checksum = snapshot_checksum(
            &start.chunk,
            start.epoch,
            start.entity_revision,
            &ordered,
            catalog,
        )
        .is_ok_and(|checksum| checksum == start.checksum);
        if !valid_checksum {
            self.snapshots.remove(&key);
            return Assembly::Resync(vec![key]);
        }
        let mut entities = BTreeMap::new();
        for page in ordered {
            for entity in page {
                if entities.insert(entity.id, entity).is_some() {
                    self.snapshots.remove(&key);
                    return Assembly::Resync(vec![key]);
                }
            }
        }
        if !self.entity_budget_with(key, &entities) {
            self.snapshots.remove(&key);
            return Assembly::Resync(vec![key]);
        }
        let Ok(avatars) = registry.project(&entities) else {
            self.snapshots.remove(&key);
            return Assembly::Resync(vec![key]);
        };
        let pending = self.snapshots.remove(&key).expect("snapshot checked above");
        let start = pending.start.expect("snapshot checked above");
        chunks.insert(key, Arc::new(start.chunk));
        self.epochs.insert(key, start.epoch);
        self.entity_revisions.insert(key, start.entity_revision);
        self.entities.insert(key, entities);
        self.install_avatars(key, avatars);
        Assembly::Installed(vec![key])
    }

    fn commit_part(
        &mut self,
        part: WorldCommitPart,
        chunks: &mut HashMap<ChunkKey, Arc<Chunk>>,
        registry: &EntityClientRegistry,
    ) -> Assembly {
        if part.commit_id <= self.last_commit {
            return Assembly::Waiting;
        }
        if self.commits.len() >= MAX_PENDING_COMMITS && !self.commits.contains_key(&part.commit_id)
        {
            return Assembly::Resync(vec![part.key]);
        }
        let incoming_len = server_wire_len(&ServerMessage::WorldCommitPart(part.clone()));
        let pending = self.commits.entry(part.commit_id).or_default();
        if pending.count == 0 {
            pending.count = part.part_count;
        }
        if pending.count != part.part_count {
            self.commits.remove(&part.commit_id);
            return Assembly::Resync(vec![part.key]);
        }
        if let Some(old) = pending.parts.get(&part.part_index) {
            if old != &part {
                self.commits.remove(&part.commit_id);
                return Assembly::Resync(vec![part.key]);
            }
            return Assembly::Waiting;
        }
        pending.bytes = pending.bytes.saturating_add(incoming_len);
        pending.parts.insert(part.part_index, part.clone());
        if pending.bytes > MAX_PENDING_BYTES || self.pending_bytes() > MAX_PENDING_BYTES {
            self.commits.remove(&part.commit_id);
            return Assembly::Resync(vec![part.key]);
        }
        self.finish_commits(chunks, registry)
    }

    fn finish_commits(
        &mut self,
        chunks: &mut HashMap<ChunkKey, Arc<Chunk>>,
        registry: &EntityClientRegistry,
    ) -> Assembly {
        let mut installed = Vec::new();
        while let Some((&id, pending)) = self.commits.first_key_value() {
            if pending.parts.len() != usize::from(pending.count) {
                break;
            }
            let pending = self.commits.remove(&id).expect("first key exists");
            match self.apply_commit(pending, chunks, registry) {
                Ok(keys) => {
                    self.last_commit = id;
                    installed.extend(keys);
                }
                Err(mut keys) => {
                    // Earlier groups in this drain may already have installed.
                    // Refresh them too so their mesh updates are not lost.
                    keys.extend(installed);
                    keys.sort_unstable_by_key(|key| (key.x, key.y, key.z));
                    keys.dedup();
                    self.commits.clear();
                    return Assembly::Resync(keys);
                }
            }
        }
        if installed.is_empty() {
            Assembly::Waiting
        } else {
            installed.sort_unstable_by_key(|key| (key.x, key.y, key.z));
            installed.dedup();
            Assembly::Installed(installed)
        }
    }

    fn apply_commit(
        &mut self,
        pending: PendingCommit,
        chunks: &mut HashMap<ChunkKey, Arc<Chunk>>,
        registry: &EntityClientRegistry,
    ) -> Result<Vec<ChunkKey>, Vec<ChunkKey>> {
        let parts: Vec<_> = pending.parts.into_values().collect();
        let keys: Vec<_> = parts.iter().map(|part| part.key).collect();
        let changed_block_keys: Vec<_> = parts
            .iter()
            .filter(|part| !part.blocks.is_empty())
            .map(|part| part.key)
            .collect();
        let mut revised_chunks = HashMap::<ChunkKey, Chunk>::new();
        let mut revised_entities = HashMap::<ChunkKey, BTreeMap<u64, PublicEntity>>::new();
        let mut revised_entity_versions = HashMap::<ChunkKey, u64>::new();
        for part in &parts {
            let Some(epoch) = self.epochs.get(&part.key) else {
                return Err(keys);
            };
            if *epoch != part.epoch {
                return Err(keys);
            }
            let Some(existing) = chunks.get(&part.key) else {
                return Err(keys);
            };
            let block_version = revised_chunks
                .get(&part.key)
                .map_or(existing.version, |chunk| chunk.version);
            let current_entity_version = revised_entity_versions
                .get(&part.key)
                .copied()
                .or_else(|| self.entity_revisions.get(&part.key).copied());
            if block_version != part.block_from || current_entity_version != Some(part.entity_from)
            {
                super::trace::event(format_args!(
                    "revision mismatch {:?}: blocks {block_version}/{} entities {:?}/{} commit {}",
                    part.key,
                    part.block_from,
                    current_entity_version,
                    part.entity_from,
                    part.commit_id
                ));
                return Err(keys);
            }
            if !part.blocks.is_empty() {
                let chunk = revised_chunks
                    .entry(part.key)
                    .or_insert_with(|| (**existing).clone());
                for change in &part.blocks {
                    let Some(index) = Chunk::index(change.local.map(usize::from)) else {
                        return Err(keys);
                    };
                    chunk.blocks.set(index, change.block);
                }
                chunk.version = part.block_to;
            }
            let entities = revised_entities
                .entry(part.key)
                .or_insert_with(|| self.entities.get(&part.key).cloned().unwrap_or_default());
            for change in &part.entities {
                match change {
                    PublicEntityChange::Upsert(entity) => {
                        if let Some(old) = entities.get(&entity.id)
                            && !valid_successor(old, entity)
                        {
                            super::trace::event(format_args!(
                                "entity mismatch {:?}: old={old:?} new={entity:?}",
                                part.key
                            ));
                            return Err(keys);
                        }
                        entities.insert(entity.id, entity.clone());
                    }
                    PublicEntityChange::Remove { id, revision } => {
                        if entities.get(id).is_some_and(|old| old.revision > *revision) {
                            return Err(keys);
                        }
                        entities.remove(id);
                    }
                }
            }
            revised_entity_versions.insert(part.key, part.entity_to);
        }
        let revised_bytes: usize = revised_entities
            .values()
            .flat_map(|entities| entities.values())
            .map(PublicEntity::wire_len)
            .sum();
        let retained_bytes: usize = self
            .entities
            .iter()
            .filter(|(key, _)| !revised_entities.contains_key(key))
            .flat_map(|(_, entities)| entities.values())
            .map(PublicEntity::wire_len)
            .sum();
        if revised_entities.values().any(|entities| {
            entities.values().map(PublicEntity::wire_len).sum::<usize>() > MAX_CHUNK_ENTITY_BYTES
        }) || retained_bytes.saturating_add(revised_bytes) > MAX_CLIENT_ENTITY_BYTES
        {
            return Err(keys);
        }
        let mut revised_avatars = HashMap::new();
        for (&key, entities) in &revised_entities {
            let avatars = registry.project(entities).map_err(|()| keys.clone())?;
            revised_avatars.insert(key, avatars);
        }
        for (key, chunk) in revised_chunks {
            chunks.insert(key, Arc::new(chunk));
        }
        for (key, entities) in revised_entities {
            self.entities.insert(key, entities);
        }
        // A cross-chunk transfer may remove and reinsert the same avatar ID.
        // Clear every preimage before installing any replacement, independent
        // of HashMap iteration order.
        for &key in revised_avatars.keys() {
            self.remove_avatars(key);
        }
        for (key, avatars) in revised_avatars {
            self.insert_avatars(key, avatars);
        }
        self.entity_revisions.extend(revised_entity_versions);
        // Entity motion changes avatar state, not chunk geometry or lighting.
        // Only block-bearing owners need a mesh/relight request.
        Ok(changed_block_keys)
    }

    fn pending_bytes(&self) -> usize {
        self.snapshots
            .values()
            .map(|pending| pending.bytes)
            .chain(self.commits.values().map(|pending| pending.bytes))
            .sum()
    }

    fn entity_budget_with(&self, key: ChunkKey, replacement: &BTreeMap<u64, PublicEntity>) -> bool {
        let replacement_bytes: usize = replacement.values().map(PublicEntity::wire_len).sum();
        if replacement_bytes > MAX_CHUNK_ENTITY_BYTES {
            return false;
        }
        let retained_bytes: usize = self
            .entities
            .iter()
            .filter(|(other, _)| **other != key)
            .flat_map(|(_, entities)| entities.values())
            .map(PublicEntity::wire_len)
            .sum();
        retained_bytes.saturating_add(replacement_bytes) <= MAX_CLIENT_ENTITY_BYTES
    }

    fn install_avatars(&mut self, key: ChunkKey, avatars: Vec<VisualAvatar>) {
        self.remove_avatars(key);
        self.insert_avatars(key, avatars);
    }

    fn remove_avatars(&mut self, key: ChunkKey) {
        if let Some(old) = self.avatar_ids_by_chunk.remove(&key) {
            for id in old {
                self.avatars.remove(&id);
            }
        }
    }

    fn insert_avatars(&mut self, key: ChunkKey, avatars: Vec<VisualAvatar>) {
        let ids: Vec<_> = avatars.iter().map(|avatar| avatar.id).collect();
        for avatar in avatars {
            self.avatars.insert(avatar.id, avatar);
        }
        self.avatar_ids_by_chunk.insert(key, ids);
    }
}

/// Mobile position has its own revision domain. A motion-only publication is
/// valid without a payload revision bump; equal revisions still forbid changing
/// identity, payload, or anchored state.
fn valid_successor(old: &PublicEntity, new: &PublicEntity) -> bool {
    if new.entity_type != old.entity_type
        || new.revision < old.revision
        || new.motion_revision < old.motion_revision
    {
        return false;
    }
    if new.revision != old.revision {
        return true;
    }
    if new.motion_revision == old.motion_revision {
        return new == old;
    }
    new.payload == old.payload
        && matches!(
            old.location,
            crate::protocol::PublicEntityLocation::Mobile { .. }
        )
        && matches!(
            new.location,
            crate::protocol::PublicEntityLocation::Mobile { .. }
        )
}

#[cfg(test)]
mod tests;

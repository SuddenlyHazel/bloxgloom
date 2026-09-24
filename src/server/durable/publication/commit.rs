//! Turn a WAL-acknowledged effect into one bounded, interested commit group.
//! Entity changes are projected from registered public views, never private payloads.

use crate::content::Catalog;
use crate::protocol::{
    BlockCellChange, MAX_FRAME, MAX_WORLD_COMMIT_BYTES, MAX_WORLD_COMMIT_PARTS, PublicEntityChange,
    ServerMessage, WorldCommitPart, server_wire_len,
};
use crate::server::Client;
use crate::server::durable::PublishEffects;
use crate::server::entities::{EntityDelta, EntityPublicView};
use crate::server::streaming::entities::project;
use crate::world::ChunkKey;
use std::collections::BTreeMap;
use std::io;

#[derive(Clone, Default)]
struct KeyChanges {
    block_version: Option<u64>,
    blocks: Vec<BlockCellChange>,
    entity_revision: Option<u64>,
    entities: Vec<PublicEntityChange>,
}

pub(super) struct CommitChanges(BTreeMap<ChunkKey, KeyChanges>);

impl CommitChanges {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn subscribed_keys(&self, client: &Client) -> Vec<ChunkKey> {
        self.0
            .keys()
            .filter(|key| client.sent.contains(key))
            .copied()
            .collect()
    }
}

pub(super) struct CommitPlan {
    pub(super) parts: Vec<WorldCommitPart>,
    pub(super) revisions: Vec<(ChunkKey, u64, u64)>,
}

/// `None` means this one client's interested subset exceeds an outbound
/// bound. The caller invalidates its subscribed keys and streams fresh epochs.
pub(super) fn for_client(
    changes: &CommitChanges,
    client: &Client,
    commit_id: u64,
) -> io::Result<Option<CommitPlan>> {
    let subscribed: Vec<_> = changes
        .0
        .iter()
        .filter(|(key, _)| client.sent.contains(key))
        .collect();
    if subscribed.is_empty() {
        return Ok(Some(CommitPlan {
            parts: Vec::new(),
            revisions: Vec::new(),
        }));
    }
    if subscribed.len() > MAX_WORLD_COMMIT_PARTS {
        return Ok(None);
    }
    let part_count = u16::try_from(subscribed.len())
        .map_err(|_| io::Error::other("world commit part count overflow"))?;
    let mut parts = Vec::with_capacity(subscribed.len());
    let mut revisions = Vec::with_capacity(subscribed.len());
    let mut total_bytes = 0usize;
    for (index, (&key, change)) in subscribed.into_iter().enumerate() {
        let epoch = *client
            .sent_epochs
            .get(&key)
            .ok_or_else(|| io::Error::other("subscribed chunk has no snapshot epoch"))?;
        let block_from = *client
            .sent_block_versions
            .get(&key)
            .ok_or_else(|| io::Error::other("subscribed chunk has no block revision"))?;
        let entity_from = *client
            .sent_entity_revisions
            .get(&key)
            .ok_or_else(|| io::Error::other("subscribed chunk has no entity revision"))?;
        let block_to = change.block_version.unwrap_or(block_from);
        let entity_to = change.entity_revision.unwrap_or(entity_from);
        if block_to < block_from || entity_to < entity_from {
            return Err(io::Error::other("committed world revision regressed"));
        }
        let part = WorldCommitPart {
            commit_id,
            part_index: index as u16,
            part_count,
            key,
            epoch,
            block_from,
            block_to,
            entity_from,
            entity_to,
            blocks: change.blocks.clone(),
            entities: change.entities.clone(),
        };
        if part.blocks.len() > crate::world::CHUNK_VOLUME || part.entities.len() > 256 {
            return Ok(None);
        }
        let bytes = server_wire_len(&ServerMessage::WorldCommitPart(part.clone()));
        total_bytes = total_bytes.saturating_add(bytes);
        if bytes > MAX_FRAME + 4 || total_bytes > MAX_WORLD_COMMIT_BYTES {
            return Ok(None);
        }
        revisions.push((key, block_to, entity_to));
        parts.push(part);
    }
    Ok(Some(CommitPlan { parts, revisions }))
}

pub(super) fn collect(effect: &PublishEffects, catalog: &Catalog) -> io::Result<CommitChanges> {
    let mut grouped = BTreeMap::<ChunkKey, KeyChanges>::new();
    for delta in &effect.deltas {
        let entry = grouped.entry(delta.key).or_default();
        if let Some(version) = entry.block_version
            && version != delta.version
        {
            return Err(io::Error::other(
                "one commit has conflicting block revisions",
            ));
        }
        if catalog.state(delta.block).is_none() {
            return Err(io::Error::other("committed block has no catalog identity"));
        }
        entry.block_version = Some(delta.version);
        entry.blocks.push(BlockCellChange {
            local: delta.local,
            block: delta.block,
        });
    }
    if let Some(commit) = &effect.entity_commit {
        for delta in &commit.deltas {
            match delta {
                EntityDelta::Spawned(view) | EntityDelta::Moved(view) => {
                    add_upsert(&mut grouped, view, commit.registry_revision, catalog)?;
                }
                EntityDelta::Updated {
                    before_touched_chunks,
                    view,
                }
                | EntityDelta::Transferred {
                    before_touched_chunks,
                    view,
                    ..
                } => {
                    add_remove(
                        &mut grouped,
                        before_touched_chunks,
                        view.id.get(),
                        view.revision,
                        commit.registry_revision,
                    );
                    add_upsert(&mut grouped, view, commit.registry_revision, catalog)?;
                }
                EntityDelta::Despawned {
                    id,
                    revision,
                    touched_chunks,
                    ..
                } => add_remove(
                    &mut grouped,
                    touched_chunks,
                    id.get(),
                    *revision,
                    commit.registry_revision,
                ),
            }
        }
    }
    Ok(CommitChanges(grouped))
}

fn add_upsert(
    grouped: &mut BTreeMap<ChunkKey, KeyChanges>,
    view: &EntityPublicView,
    registry_revision: u64,
    catalog: &Catalog,
) -> io::Result<()> {
    let public = project(view.clone());
    public.validate(catalog)?;
    for key in view.location.touched_chunks().map_err(io::Error::other)? {
        let entry = grouped.entry(key).or_default();
        entry.entity_revision = Some(registry_revision);
        entry
            .entities
            .push(PublicEntityChange::Upsert(public.clone()));
    }
    Ok(())
}

fn add_remove(
    grouped: &mut BTreeMap<ChunkKey, KeyChanges>,
    chunks: &[ChunkKey],
    id: u64,
    revision: u64,
    registry_revision: u64,
) {
    for &key in chunks {
        let entry = grouped.entry(key).or_default();
        entry.entity_revision = Some(registry_revision);
        entry
            .entities
            .push(PublicEntityChange::Remove { id, revision });
    }
}

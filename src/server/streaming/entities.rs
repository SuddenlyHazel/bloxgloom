//! Registered public projection of one authoritative chunk and its sparse entities.

use crate::content::Catalog;
use crate::protocol::{
    EntitySnapshotPage, MAX_ENTITY_SNAPSHOT_PAGES, MAX_FRAME, PublicEntity, PublicEntityLocation,
    ServerMessage, WorldSnapshotStart, server_wire_len, snapshot_checksum,
};
use crate::server::entities::{EntityLocation, EntityPublicView};
use crate::world::Chunk;
use std::io;

pub(super) const MAX_PUBLIC_ENTITIES_PER_CHUNK: usize = 1024;
pub(super) const MAX_PUBLIC_ENTITY_BYTES_PER_CHUNK: usize = 1024 * 1024;

#[derive(Debug)]
pub(super) enum SnapshotError {
    Capacity,
    Invalid(io::Error),
}

impl From<io::Error> for SnapshotError {
    fn from(error: io::Error) -> Self {
        Self::Invalid(error)
    }
}

pub(super) fn snapshot_messages(
    chunk: Chunk,
    epoch: u64,
    entity_revision: u64,
    views: Vec<EntityPublicView>,
    catalog: &Catalog,
) -> Result<Vec<ServerMessage>, SnapshotError> {
    if views.len() > MAX_PUBLIC_ENTITIES_PER_CHUNK {
        return Err(SnapshotError::Capacity);
    }
    let mut entities = views
        .into_iter()
        .map(project)
        .collect::<Vec<PublicEntity>>();
    entities.sort_unstable_by_key(|entity| entity.id);
    if entities.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err(io::Error::other("duplicate public entity in chunk").into());
    }
    let mut pages = Vec::<Vec<PublicEntity>>::new();
    let mut public_bytes = 0usize;
    let page_header = server_wire_len(&ServerMessage::EntitySnapshotPage(EntitySnapshotPage {
        key: chunk.key,
        epoch,
        entity_revision,
        page_index: 0,
        page_count: 1,
        checksum: 0,
        entities: Vec::new(),
    }));
    let mut page_bytes = page_header;
    for entity in entities {
        entity.validate(catalog)?;
        let entity_bytes = entity.wire_len();
        public_bytes = public_bytes
            .checked_add(entity_bytes)
            .ok_or_else(|| io::Error::other("public entity byte count overflow"))?;
        if public_bytes > MAX_PUBLIC_ENTITY_BYTES_PER_CHUNK {
            return Err(SnapshotError::Capacity);
        }
        if page_header + entity_bytes > MAX_FRAME + 4 {
            return Err(SnapshotError::Capacity);
        }
        if pages.is_empty() {
            pages.push(Vec::new());
        }
        let last = pages.last_mut().expect("nonempty after push");
        if last.len() == 256 || page_bytes + entity_bytes > MAX_FRAME + 4 {
            pages.push(Vec::new());
            page_bytes = page_header;
        }
        pages.last_mut().expect("page exists").push(entity);
        page_bytes += entity_bytes;
        if pages.len() > MAX_ENTITY_SNAPSHOT_PAGES {
            return Err(SnapshotError::Capacity);
        }
    }
    let checksum = snapshot_checksum(&chunk, epoch, entity_revision, &pages, catalog)?;
    let page_count = u16::try_from(pages.len())
        .map_err(|_| io::Error::other("public entity page count overflow"))?;
    let key = chunk.key;
    let start = WorldSnapshotStart {
        chunk,
        epoch,
        entity_revision,
        entity_page_count: page_count,
        checksum,
    };
    if server_wire_len(&ServerMessage::WorldSnapshotStart(start.clone())) > MAX_FRAME + 4 {
        return Err(SnapshotError::Capacity);
    }
    let mut messages = Vec::with_capacity(pages.len() + 1);
    messages.push(ServerMessage::WorldSnapshotStart(start));
    for (index, entities) in pages.into_iter().enumerate() {
        messages.push(ServerMessage::EntitySnapshotPage(EntitySnapshotPage {
            key,
            epoch,
            entity_revision,
            page_index: index as u16,
            page_count,
            checksum,
            entities,
        }));
    }
    Ok(messages)
}

pub(in crate::server) fn project(view: EntityPublicView) -> PublicEntity {
    let location = match view.location {
        EntityLocation::Mobile { position } => PublicEntityLocation::Mobile { position },
        EntityLocation::Anchored {
            anchor,
            anchor_state,
            ..
        } => PublicEntityLocation::Anchored {
            anchor: [anchor.x, anchor.y, anchor.z],
            anchor_state,
        },
    };
    PublicEntity {
        id: view.id.get(),
        entity_type: view.entity_type,
        revision: view.revision,
        motion_revision: view.motion_revision,
        location,
        payload: view.payload,
    }
}

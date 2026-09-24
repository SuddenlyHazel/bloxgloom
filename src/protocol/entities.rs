//! Bounded public-entity wire records. Private entity payloads and footprints
//! never enter these messages; the server projects only registered public views.

use super::{Catalog, Cursor, MAX_FRAME, invalid};
use crate::content::{BlockStateId, EntityTypeId};
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey};
use std::io;

pub const MAX_PUBLIC_ENTITY_PAYLOAD: usize = 4 * 1024;
pub const MAX_ENTITY_SNAPSHOT_PAGES: usize = 64;
pub const MAX_ENTITY_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ENTITIES_PER_PAGE: usize = 256;
pub const MAX_WORLD_COMMIT_PARTS: usize = 512;
pub const MAX_WORLD_COMMIT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ENTITY_CHANGES_PER_PART: usize = 256;
pub const MAX_BLOCK_CHANGES_PER_PART: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

#[derive(Clone, Debug, PartialEq)]
pub enum PublicEntityLocation {
    Mobile {
        position: [f32; 3],
    },
    Anchored {
        anchor: [i32; 3],
        anchor_state: BlockStateId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PublicEntity {
    pub id: u64,
    pub entity_type: EntityTypeId,
    pub revision: u64,
    pub motion_revision: u64,
    pub location: PublicEntityLocation,
    pub payload: Vec<u8>,
}

impl PublicEntity {
    pub fn validate(&self, catalog: &Catalog) -> io::Result<()> {
        if self.id == 0
            || self.revision == 0
            || self.payload.len() > MAX_PUBLIC_ENTITY_PAYLOAD
            || catalog.entity_type(self.entity_type).is_none()
        {
            return Err(invalid("invalid public entity"));
        }
        match &self.location {
            PublicEntityLocation::Mobile { position } => {
                if self.motion_revision == 0 || position.iter().any(|axis| !axis.is_finite()) {
                    return Err(invalid("invalid mobile public entity"));
                }
            }
            PublicEntityLocation::Anchored { anchor_state, .. } => {
                if self.motion_revision != 0 || catalog.state(*anchor_state).is_none() {
                    return Err(invalid("invalid anchored public entity"));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn wire_len(&self) -> usize {
        8 + 4
            + 8
            + 8
            + 1
            + 12
            + 2
            + usize::from(matches!(
                self.location,
                PublicEntityLocation::Anchored { .. }
            )) * 4
            + self.payload.len()
    }
}

#[derive(Clone, Debug)]
pub struct WorldSnapshotStart {
    pub chunk: Chunk,
    pub epoch: u64,
    pub entity_revision: u64,
    pub entity_page_count: u16,
    pub checksum: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntitySnapshotPage {
    pub key: ChunkKey,
    pub epoch: u64,
    pub entity_revision: u64,
    pub page_index: u16,
    pub page_count: u16,
    pub checksum: u64,
    pub entities: Vec<PublicEntity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockCellChange {
    pub local: [u8; 3],
    pub block: BlockStateId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PublicEntityChange {
    Upsert(PublicEntity),
    Remove { id: u64, revision: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldCommitPart {
    pub commit_id: u64,
    pub part_index: u16,
    pub part_count: u16,
    pub key: ChunkKey,
    pub epoch: u64,
    pub block_from: u64,
    pub block_to: u64,
    pub entity_from: u64,
    pub entity_to: u64,
    pub blocks: Vec<BlockCellChange>,
    pub entities: Vec<PublicEntityChange>,
}

impl WorldCommitPart {
    fn validate(&self, catalog: &Catalog) -> io::Result<()> {
        if self.commit_id == 0
            || self.epoch == 0
            || self.part_count == 0
            || usize::from(self.part_count) > MAX_WORLD_COMMIT_PARTS
            || self.part_index >= self.part_count
            || self.blocks.len() > MAX_BLOCK_CHANGES_PER_PART
            || self.entities.len() > MAX_ENTITY_CHANGES_PER_PART
            || (self.blocks.is_empty() && self.entities.is_empty())
            || self.block_to < self.block_from
            || self.entity_to < self.entity_from
            || self.blocks.is_empty() != (self.block_to == self.block_from)
            || self.entities.is_empty() != (self.entity_to == self.entity_from)
            || (!self.blocks.is_empty() && self.block_from.checked_add(1) != Some(self.block_to))
        {
            return Err(invalid("invalid world commit part"));
        }
        for cell in &self.blocks {
            if cell
                .local
                .iter()
                .any(|axis| usize::from(*axis) >= CHUNK_SIZE)
                || catalog.state(cell.block).is_none()
            {
                return Err(invalid("invalid world commit block"));
            }
        }
        for change in &self.entities {
            match change {
                PublicEntityChange::Upsert(view) => view.validate(catalog)?,
                PublicEntityChange::Remove { id, revision } if *id != 0 && *revision != 0 => {}
                PublicEntityChange::Remove { .. } => {
                    return Err(invalid("invalid world commit removal"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn wire_len(&self) -> usize {
        // The length prefix and version/tag live in the caller.
        8 + 2
            + 2
            + 12
            + 8 * 5
            + 2
            + self.blocks.len() * 7
            + 2
            + self
                .entities
                .iter()
                .map(|change| match change {
                    PublicEntityChange::Upsert(entity) => 1 + entity.wire_len(),
                    PublicEntityChange::Remove { .. } => 1 + 8 + 8,
                })
                .sum::<usize>()
    }
}

pub(super) fn snapshot_start_wire_len(start: &WorldSnapshotStart) -> usize {
    let unique = start.chunk.blocks.unique_states();
    // key, block revision, epoch, entity revision, page count, checksum, palette.
    12 + 8
        + 8
        + 8
        + 2
        + 8
        + 2
        + 1
        + unique * 4
        + CHUNK_SIZE.pow(3) * if unique <= 256 { 1 } else { 2 }
}

pub(super) fn snapshot_page_wire_len(page: &EntitySnapshotPage) -> usize {
    12 + 8
        + 8
        + 2
        + 2
        + 8
        + 2
        + page
            .entities
            .iter()
            .map(PublicEntity::wire_len)
            .sum::<usize>()
}

pub(super) fn write_snapshot_start(
    out: &mut Vec<u8>,
    start: &WorldSnapshotStart,
    catalog: &Catalog,
) -> io::Result<()> {
    if start.epoch == 0
        || usize::from(start.entity_page_count) > MAX_ENTITY_SNAPSHOT_PAGES
        || start.chunk.blocks.len() != CHUNK_SIZE.pow(3)
        || start
            .chunk
            .blocks
            .iter()
            .any(|block| catalog.state(*block).is_none())
    {
        return Err(invalid("invalid world snapshot start"));
    }
    super::key(out, start.chunk.key);
    out.extend(start.chunk.version.to_le_bytes());
    out.extend(start.epoch.to_le_bytes());
    out.extend(start.entity_revision.to_le_bytes());
    out.extend(start.entity_page_count.to_le_bytes());
    out.extend(start.checksum.to_le_bytes());
    super::write_paletted_blocks(out, &start.chunk.blocks)
}

pub(super) fn read_snapshot_start(
    c: &mut Cursor<'_>,
    catalog: &Catalog,
) -> io::Result<WorldSnapshotStart> {
    let key = c.key()?;
    let block_revision = c.u64()?;
    let epoch = c.u64()?;
    let entity_revision = c.u64()?;
    let entity_page_count = c.u16()?;
    let checksum = c.u64()?;
    if epoch == 0 || usize::from(entity_page_count) > MAX_ENTITY_SNAPSHOT_PAGES {
        return Err(invalid("invalid world snapshot start"));
    }
    let blocks = super::read_palette_with(c, |state| catalog.state(state).is_some())?;
    Ok(WorldSnapshotStart {
        chunk: Chunk::from_blocks(key, block_revision, blocks),
        epoch,
        entity_revision,
        entity_page_count,
        checksum,
    })
}

pub(super) fn write_snapshot_page(
    out: &mut Vec<u8>,
    page: &EntitySnapshotPage,
    catalog: &Catalog,
) -> io::Result<()> {
    validate_page(page, catalog)?;
    super::key(out, page.key);
    out.extend(page.epoch.to_le_bytes());
    out.extend(page.entity_revision.to_le_bytes());
    out.extend(page.page_index.to_le_bytes());
    out.extend(page.page_count.to_le_bytes());
    out.extend(page.checksum.to_le_bytes());
    out.extend((page.entities.len() as u16).to_le_bytes());
    for entity in &page.entities {
        write_entity(out, entity);
    }
    Ok(())
}

pub(super) fn read_snapshot_page(
    c: &mut Cursor<'_>,
    catalog: &Catalog,
) -> io::Result<EntitySnapshotPage> {
    let key = c.key()?;
    let epoch = c.u64()?;
    let entity_revision = c.u64()?;
    let page_index = c.u16()?;
    let page_count = c.u16()?;
    let checksum = c.u64()?;
    let count = usize::from(c.u16()?);
    if count > MAX_ENTITIES_PER_PAGE {
        return Err(invalid("too many public entities in page"));
    }
    let mut entities = Vec::with_capacity(count);
    for _ in 0..count {
        entities.push(read_entity(c, catalog)?);
    }
    let page = EntitySnapshotPage {
        key,
        epoch,
        entity_revision,
        page_index,
        page_count,
        checksum,
        entities,
    };
    validate_page(&page, catalog)?;
    Ok(page)
}

fn validate_page(page: &EntitySnapshotPage, catalog: &Catalog) -> io::Result<()> {
    if page.epoch == 0
        || page.page_count == 0
        || usize::from(page.page_count) > MAX_ENTITY_SNAPSHOT_PAGES
        || page.page_index >= page.page_count
        || page.entities.len() > MAX_ENTITIES_PER_PAGE
    {
        return Err(invalid("invalid entity snapshot page"));
    }
    for entity in &page.entities {
        entity.validate(catalog)?;
    }
    Ok(())
}

pub(super) fn write_commit_part(
    out: &mut Vec<u8>,
    part: &WorldCommitPart,
    catalog: &Catalog,
) -> io::Result<()> {
    part.validate(catalog)?;
    out.extend(part.commit_id.to_le_bytes());
    out.extend(part.part_index.to_le_bytes());
    out.extend(part.part_count.to_le_bytes());
    super::key(out, part.key);
    out.extend(part.epoch.to_le_bytes());
    for revision in [
        part.block_from,
        part.block_to,
        part.entity_from,
        part.entity_to,
    ] {
        out.extend(revision.to_le_bytes());
    }
    out.extend((part.blocks.len() as u16).to_le_bytes());
    for change in &part.blocks {
        out.extend(change.local);
        out.extend(change.block.get().to_le_bytes());
    }
    out.extend((part.entities.len() as u16).to_le_bytes());
    for change in &part.entities {
        match change {
            PublicEntityChange::Upsert(entity) => {
                out.push(1);
                write_entity(out, entity);
            }
            PublicEntityChange::Remove { id, revision } => {
                out.push(2);
                out.extend(id.to_le_bytes());
                out.extend(revision.to_le_bytes());
            }
        }
    }
    Ok(())
}

pub(super) fn read_commit_part(
    c: &mut Cursor<'_>,
    catalog: &Catalog,
) -> io::Result<WorldCommitPart> {
    let commit_id = c.u64()?;
    let part_index = c.u16()?;
    let part_count = c.u16()?;
    let key = c.key()?;
    let epoch = c.u64()?;
    let block_from = c.u64()?;
    let block_to = c.u64()?;
    let entity_from = c.u64()?;
    let entity_to = c.u64()?;
    let block_count = usize::from(c.u16()?);
    if block_count > MAX_BLOCK_CHANGES_PER_PART {
        return Err(invalid("too many block changes in commit"));
    }
    let mut blocks = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        blocks.push(BlockCellChange {
            local: [c.u8()?, c.u8()?, c.u8()?],
            block: BlockStateId(c.u32()?),
        });
    }
    let entity_count = usize::from(c.u16()?);
    if entity_count > MAX_ENTITY_CHANGES_PER_PART {
        return Err(invalid("too many entity changes in commit"));
    }
    let mut entities = Vec::with_capacity(entity_count);
    for _ in 0..entity_count {
        entities.push(match c.u8()? {
            1 => PublicEntityChange::Upsert(read_entity(c, catalog)?),
            2 => PublicEntityChange::Remove {
                id: c.u64()?,
                revision: c.u64()?,
            },
            _ => return Err(invalid("invalid entity change type")),
        });
    }
    let part = WorldCommitPart {
        commit_id,
        part_index,
        part_count,
        key,
        epoch,
        block_from,
        block_to,
        entity_from,
        entity_to,
        blocks,
        entities,
    };
    part.validate(catalog)?;
    Ok(part)
}

fn write_entity(out: &mut Vec<u8>, entity: &PublicEntity) {
    out.extend(entity.id.to_le_bytes());
    out.extend(entity.entity_type.get().to_le_bytes());
    out.extend(entity.revision.to_le_bytes());
    out.extend(entity.motion_revision.to_le_bytes());
    match &entity.location {
        PublicEntityLocation::Mobile { position } => {
            out.push(1);
            for axis in position {
                out.extend(axis.to_le_bytes());
            }
        }
        PublicEntityLocation::Anchored {
            anchor,
            anchor_state,
        } => {
            out.push(2);
            for axis in anchor {
                out.extend(axis.to_le_bytes());
            }
            out.extend(anchor_state.get().to_le_bytes());
        }
    }
    out.extend((entity.payload.len() as u16).to_le_bytes());
    out.extend(&entity.payload);
}

fn read_entity(c: &mut Cursor<'_>, catalog: &Catalog) -> io::Result<PublicEntity> {
    let id = c.u64()?;
    let entity_type = EntityTypeId(c.u32()?);
    let revision = c.u64()?;
    let motion_revision = c.u64()?;
    let location = match c.u8()? {
        1 => PublicEntityLocation::Mobile {
            position: [c.f32()?, c.f32()?, c.f32()?],
        },
        2 => PublicEntityLocation::Anchored {
            anchor: [c.i32()?, c.i32()?, c.i32()?],
            anchor_state: BlockStateId(c.u32()?),
        },
        _ => return Err(invalid("invalid public entity location")),
    };
    let payload_len = usize::from(c.u16()?);
    if payload_len > MAX_PUBLIC_ENTITY_PAYLOAD {
        return Err(invalid("public entity payload too large"));
    }
    let entity = PublicEntity {
        id,
        entity_type,
        revision,
        motion_revision,
        location,
        payload: c.take(payload_len)?.to_vec(),
    };
    entity.validate(catalog)?;
    Ok(entity)
}

/// One checksum binds a chunk and its ordered public-entity continuation pages.
pub fn snapshot_checksum(
    chunk: &Chunk,
    epoch: u64,
    entity_revision: u64,
    pages: &[Vec<PublicEntity>],
    catalog: &Catalog,
) -> io::Result<u64> {
    if epoch == 0 || pages.len() > MAX_ENTITY_SNAPSHOT_PAGES {
        return Err(invalid("invalid snapshot checksum input"));
    }
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for coordinate in [chunk.key.x, chunk.key.y, chunk.key.z] {
        feed(&coordinate.to_le_bytes());
    }
    feed(&chunk.version.to_le_bytes());
    feed(&epoch.to_le_bytes());
    feed(&entity_revision.to_le_bytes());
    feed(&(pages.len() as u16).to_le_bytes());
    for block in chunk.blocks.iter() {
        if catalog.state(*block).is_none() {
            return Err(invalid("invalid snapshot block"));
        }
        feed(&block.get().to_le_bytes());
    }
    let mut total = 0usize;
    for (index, page) in pages.iter().enumerate() {
        if page.len() > MAX_ENTITIES_PER_PAGE {
            return Err(invalid("too many entities in checksum page"));
        }
        feed(&(index as u16).to_le_bytes());
        feed(&(page.len() as u16).to_le_bytes());
        for entity in page {
            entity.validate(catalog)?;
            let mut encoded = Vec::with_capacity(entity.wire_len());
            write_entity(&mut encoded, entity);
            total = total
                .checked_add(encoded.len())
                .ok_or_else(|| invalid("entity snapshot size overflow"))?;
            if total > MAX_ENTITY_SNAPSHOT_BYTES {
                return Err(invalid("entity snapshot exceeds assembly bound"));
            }
            feed(&encoded);
        }
    }
    Ok(hash)
}

pub(super) fn enforce_frame_size(out: &[u8]) -> io::Result<()> {
    if out.len() > MAX_FRAME {
        return Err(invalid("entity frame exceeds 64 KiB"));
    }
    Ok(())
}

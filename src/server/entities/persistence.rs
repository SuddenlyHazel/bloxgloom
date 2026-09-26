use super::codec::{
    Decoder, ENTITY_ALLOCATOR_MAGIC, ENTITY_ALLOCATOR_VERSION, ENTITY_MOTION_MAGIC,
    ENTITY_MOTION_VERSION, ENTITY_RECORD_MAGIC, ENTITY_RECORD_VERSION, ENTITY_WAL_RECORD_MAGIC,
    ENTITY_WAL_RECORD_VERSION, Encoder, checked_body,
};
use super::registry::{EntityTypeDescriptor, EntityTypeRegistry};
use super::spatial::{
    ChunkPage, decode_cell_key, decode_cell_owner, decode_chunk_key, encode_cell_key,
    encode_cell_owner, encode_chunk_key,
};
use super::store::{EntityRecord, EntityStore, MAX_ENTITY_RECORDS};
use super::types::{
    CellCoord, EntityError, EntityId, EntityLocation, EntityMotionSnapshot, EntityOwner,
    EntityPayload, MAX_ENTITY_FOOTPRINT_CELLS, MAX_ENTITY_PAYLOAD_BYTES,
};
use crate::content::{BlockStateId, EntityTypeId};
use crate::server::journal::StateKey;
use crate::world::ChunkKey;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod stream;
pub(in crate::server) use stream::write_checkpoint;

const CHECKPOINT_MAGIC: &[u8; 4] = b"BGEN";
const CHECKPOINT_VERSION: u16 = 3;
const MIN_SUPPORTED_CHECKPOINT_VERSION: u16 = 2;
const ENTITY_REVISION_MAGIC: &[u8; 4] = b"BGEV";
const ENTITY_REVISION_VERSION: u16 = 1;
pub(super) const MAX_ENTITY_SNAPSHOT_BYTES: usize = 256 * 1024 * 1024;
const MAX_CHECKPOINT_PAGES: usize = 1_048_576;
const MAX_CHECKPOINT_CELLS: usize = 16_777_216;
const MAX_ENTITY_RECORD_VALUE_BYTES: usize = 256 * 1024;

pub fn encode_checkpoint(store: &EntityStore) -> Result<Vec<u8>, EntityError> {
    if store.len() > MAX_ENTITY_RECORDS
        || store.chunk_pages().len() > MAX_CHECKPOINT_PAGES
        || store.indexes().anchored_cells.len() > MAX_CHECKPOINT_CELLS
    {
        return Err(EntityError::TooManyEntities);
    }
    let mut encoder = Encoder::with_capacity(64 * 1024);
    encoder.raw(CHECKPOINT_MAGIC);
    encoder.u16(CHECKPOINT_VERSION);
    encoder.u64(store.next_id());
    encoder.u64(store.revision());
    encoder.u64(store.durable_sequence());
    encoder.u64(store.durable_global_revision());
    encoder.u32(u32::try_from(store.len()).map_err(|_| EntityError::TooManyEntities)?);
    encoder
        .u32(u32::try_from(store.chunk_pages().len()).map_err(|_| EntityError::TooManyEntities)?);
    encoder.u32(
        u32::try_from(store.indexes().anchored_cells.len())
            .map_err(|_| EntityError::TooManyEntities)?,
    );
    enforce_size(&encoder)?;
    for record in store.record_values() {
        let value = encode_record_value(record, store.types())?;
        encoder.length_bytes(&value)?;
        enforce_size(&encoder)?;
    }
    for (chunk, page) in store.chunk_pages() {
        encoder.raw(&chunk.x.to_le_bytes());
        encoder.raw(&chunk.y.to_le_bytes());
        encoder.raw(&chunk.z.to_le_bytes());
        encoder.length_bytes(&page.encode_value()?)?;
        enforce_size(&encoder)?;
    }
    for (cell, id) in &store.indexes().anchored_cells {
        encoder.raw(&cell.x.to_le_bytes());
        encoder.raw(&cell.y.to_le_bytes());
        encoder.raw(&cell.z.to_le_bytes());
        encoder.length_bytes(&encode_cell_owner(Some(*id))?)?;
        enforce_size(&encoder)?;
    }
    encoder.finish_crc()
}

pub fn decode_checkpoint(
    bytes: &[u8],
    types: Arc<EntityTypeRegistry>,
) -> Result<EntityStore, EntityError> {
    let body = checked_body(bytes, MAX_ENTITY_SNAPSHOT_BYTES)?;
    let mut decoder = Decoder::new(body);
    if decoder.raw(4)? != CHECKPOINT_MAGIC {
        return Err(EntityError::CorruptCheckpoint);
    }
    let version = decoder.u16()?;
    if !(MIN_SUPPORTED_CHECKPOINT_VERSION..=CHECKPOINT_VERSION).contains(&version) {
        return Err(EntityError::CorruptCheckpoint);
    }
    let next_id = decoder.u64()?;
    let revision = decoder.u64()?;
    let (durable_sequence, durable_global_revision) = if version >= 3 {
        (decoder.u64()?, decoder.u64()?)
    } else {
        (0, 0)
    };
    let record_count =
        usize::try_from(decoder.u32()?).map_err(|_| EntityError::CorruptCheckpoint)?;
    let page_count = usize::try_from(decoder.u32()?).map_err(|_| EntityError::CorruptCheckpoint)?;
    let cell_count = usize::try_from(decoder.u32()?).map_err(|_| EntityError::CorruptCheckpoint)?;
    if next_id == 0
        || record_count > MAX_ENTITY_RECORDS
        || page_count > MAX_CHECKPOINT_PAGES
        || cell_count > MAX_CHECKPOINT_CELLS
    {
        return Err(EntityError::CorruptCheckpoint);
    }
    let mut records = BTreeMap::new();
    for _ in 0..record_count {
        let bytes = decoder.length_bytes(MAX_ENTITY_RECORD_VALUE_BYTES)?;
        let record = decode_record_value_bytes(bytes, &types)?;
        if records.insert(record.id, record).is_some() {
            return Err(EntityError::CorruptCheckpoint);
        }
    }
    let mut pages = BTreeMap::<ChunkKey, ChunkPage>::new();
    let mut previous_chunk = None;
    for _ in 0..page_count {
        let chunk = ChunkKey {
            x: decoder.i32()?,
            y: decoder.i32()?,
            z: decoder.i32()?,
        };
        if previous_chunk.is_some_and(|previous| previous >= chunk) {
            return Err(EntityError::CorruptCheckpoint);
        }
        previous_chunk = Some(chunk);
        let page_bytes = decoder.length_bytes(1_048_576)?;
        let page = ChunkPage::decode_value(page_bytes)?;
        if page.entity_ids.is_empty() || pages.insert(chunk, page).is_some() {
            return Err(EntityError::CorruptCheckpoint);
        }
    }
    let mut cells = BTreeMap::new();
    let mut previous_cell = None;
    for _ in 0..cell_count {
        let cell = CellCoord::new(decoder.i32()?, decoder.i32()?, decoder.i32()?);
        if previous_cell.is_some_and(|previous| previous >= cell) {
            return Err(EntityError::CorruptCheckpoint);
        }
        previous_cell = Some(cell);
        let value = decoder.length_bytes(32)?;
        let Some(id) = decode_cell_owner(value)? else {
            return Err(EntityError::CorruptCheckpoint);
        };
        if cells.insert(cell, id).is_some() {
            return Err(EntityError::CorruptCheckpoint);
        }
    }
    decoder.finish()?;

    let store = EntityStore::from_parts(
        types,
        next_id,
        revision,
        durable_sequence,
        durable_global_revision,
        records,
    )?;
    if store.chunk_pages() != &pages || store.indexes().anchored_cells != cells {
        return Err(EntityError::CorruptCheckpoint);
    }
    Ok(store)
}

/// Validate the checkpoint envelope without resolving type-specific payloads.
/// Callers that accept the snapshot as live state must also run
/// `decode_checkpoint` with the startup-frozen type registry.
pub(super) fn validate_checkpoint_frame(bytes: &[u8]) -> Result<(), EntityError> {
    let body = checked_body(bytes, MAX_ENTITY_SNAPSHOT_BYTES)?;
    let mut decoder = Decoder::new(body);
    if decoder.raw(4)? != CHECKPOINT_MAGIC {
        return Err(EntityError::CorruptCheckpoint);
    }
    let version = decoder.u16()?;
    if !(MIN_SUPPORTED_CHECKPOINT_VERSION..=CHECKPOINT_VERSION).contains(&version) {
        return Err(EntityError::CorruptCheckpoint);
    }
    Ok(())
}

pub fn encode_revision_value(sequence: u64, global_revision: u64) -> Result<Vec<u8>, EntityError> {
    if global_revision < sequence {
        return Err(EntityError::InvalidTransaction);
    }
    // The all-zero watermark is the absent initial WAL value. Keeping it
    // empty lets legacy checkpoints enter this domain without claiming an
    // earlier BGEV value in the journal's full-key history.
    if sequence == 0 && global_revision == 0 {
        return Ok(Vec::new());
    }
    let mut encoder = Encoder::with_capacity(22);
    encoder.raw(ENTITY_REVISION_MAGIC);
    encoder.u16(ENTITY_REVISION_VERSION);
    encoder.u64(sequence);
    encoder.u64(global_revision);
    encoder.finish_crc()
}

pub fn decode_revision_value(bytes: &[u8]) -> Result<(u64, u64), EntityError> {
    if bytes.is_empty() {
        return Ok((0, 0));
    }
    let body = checked_body(bytes, 64)?;
    let mut decoder = Decoder::new(body);
    if decoder.raw(4)? != ENTITY_REVISION_MAGIC || decoder.u16()? != ENTITY_REVISION_VERSION {
        return Err(EntityError::CorruptCheckpoint);
    }
    let sequence = decoder.u64()?;
    let global_revision = decoder.u64()?;
    decoder.finish()?;
    if global_revision < sequence {
        return Err(EntityError::CorruptCheckpoint);
    }
    Ok((sequence, global_revision))
}

pub fn encode_record_value(
    record: &EntityRecord,
    types: &EntityTypeRegistry,
) -> Result<Vec<u8>, EntityError> {
    let mut encoder = Encoder::with_capacity(96 + record.payload_size);
    encoder.raw(ENTITY_RECORD_MAGIC);
    encoder.u16(ENTITY_RECORD_VERSION);
    encoder.u64(record.id.get());
    encoder.u32(record.entity_type.0);
    encoder.u16(record.schema_version);
    encoder.u64(record.schema_fingerprint);
    encoder.u64(record.revision);
    encoder.u64(record.motion_revision);
    encode_owner(&mut encoder, record.owner);
    match &record.location {
        EntityLocation::Mobile { position } => {
            encoder.u8(0);
            for coordinate in position {
                encoder.f32(*coordinate);
            }
        }
        EntityLocation::Anchored {
            anchor,
            anchor_state,
            footprint,
        } => {
            encoder.u8(1);
            encode_cell(&mut encoder, *anchor);
            encoder.u32(anchor_state.0);
            encoder.u16(u16::try_from(footprint.len()).map_err(|_| EntityError::InvalidLocation)?);
            for cell in footprint {
                encode_cell(&mut encoder, *cell);
            }
        }
    }
    encode_schedule_and_payload(&mut encoder, record, types)?;
    let value = encoder.finish_crc()?;
    if value.len() > MAX_ENTITY_RECORD_VALUE_BYTES {
        return Err(EntityError::PayloadTooLarge);
    }
    Ok(value)
}

/// WAL value for WAL-owned fields only. Mobile position is deliberately not
/// encoded here; it is checkpoint-owned except for spawn/owner-transfer
/// motion values, which use `encode_motion_value` in the same transaction.
pub fn encode_durable_record_value(
    record: &EntityRecord,
    types: &EntityTypeRegistry,
) -> Result<Vec<u8>, EntityError> {
    let mut encoder = Encoder::with_capacity(96 + record.payload_size);
    encoder.raw(ENTITY_WAL_RECORD_MAGIC);
    encoder.u16(ENTITY_WAL_RECORD_VERSION);
    encoder.u64(record.id.get());
    encoder.u32(record.entity_type.0);
    encoder.u16(record.schema_version);
    encoder.u64(record.schema_fingerprint);
    encoder.u64(record.revision);
    encode_owner(&mut encoder, record.owner);
    match &record.location {
        EntityLocation::Mobile { .. } => encoder.u8(0),
        EntityLocation::Anchored {
            anchor,
            anchor_state,
            footprint,
        } => {
            encoder.u8(1);
            encode_cell(&mut encoder, *anchor);
            encoder.u32(anchor_state.0);
            encoder.u16(u16::try_from(footprint.len()).map_err(|_| EntityError::InvalidLocation)?);
            for cell in footprint {
                encode_cell(&mut encoder, *cell);
            }
        }
    }
    encode_schedule_and_payload(&mut encoder, record, types)?;
    let value = encoder.finish_crc()?;
    if value.len() > MAX_ENTITY_RECORD_VALUE_BYTES {
        return Err(EntityError::PayloadTooLarge);
    }
    Ok(value)
}

pub fn encode_motion_value(record: &EntityRecord) -> Result<Vec<u8>, EntityError> {
    let EntityLocation::Mobile { position } = &record.location else {
        return Err(EntityError::WrongOwnership);
    };
    if record.motion_revision == 0 || position.iter().any(|value| !value.is_finite()) {
        return Err(EntityError::InvalidLocation);
    }
    let mut encoder = Encoder::with_capacity(26);
    encoder.raw(ENTITY_MOTION_MAGIC);
    encoder.u16(ENTITY_MOTION_VERSION);
    encoder.u64(record.motion_revision);
    for coordinate in position {
        encoder.f32(*coordinate);
    }
    encoder.finish_crc()
}

pub fn decode_motion_value(
    id: EntityId,
    bytes: &[u8],
) -> Result<EntityMotionSnapshot, EntityError> {
    let body = checked_body(bytes, 64)?;
    let mut decoder = Decoder::new(body);
    if decoder.raw(4)? != ENTITY_MOTION_MAGIC || decoder.u16()? != ENTITY_MOTION_VERSION {
        return Err(EntityError::CorruptCheckpoint);
    }
    let revision = decoder.u64()?;
    let position = [decoder.f32()?, decoder.f32()?, decoder.f32()?];
    decoder.finish()?;
    if revision == 0 || position.iter().any(|value| !value.is_finite()) {
        return Err(EntityError::CorruptCheckpoint);
    }
    Ok(EntityMotionSnapshot {
        id,
        revision,
        position,
    })
}

pub fn decode_record_value(
    expected_id: EntityId,
    bytes: &[u8],
    types: &EntityTypeRegistry,
) -> Result<EntityRecord, EntityError> {
    let record = decode_record_value_bytes(bytes, types)?;
    if record.id != expected_id {
        return Err(EntityError::CorruptCheckpoint);
    }
    Ok(record)
}

pub fn decode_durable_record_value(
    expected_id: EntityId,
    bytes: &[u8],
    types: &EntityTypeRegistry,
    base: Option<&EntityRecord>,
    wal_motion: Option<EntityMotionSnapshot>,
) -> Result<EntityRecord, EntityError> {
    let body = checked_body(bytes, MAX_ENTITY_RECORD_VALUE_BYTES)?;
    let mut decoder = Decoder::new(body);
    if decoder.raw(4)? != ENTITY_WAL_RECORD_MAGIC || decoder.u16()? != ENTITY_WAL_RECORD_VERSION {
        return Err(EntityError::CorruptCheckpoint);
    }
    let id = EntityId::new(decoder.u64()?).ok_or(EntityError::CorruptCheckpoint)?;
    if id != expected_id || base.is_some_and(|record| record.id != id) {
        return Err(EntityError::CorruptCheckpoint);
    }
    let entity_type = EntityTypeId(decoder.u32()?);
    let stored_schema_version = decoder.u16()?;
    let stored_schema_fingerprint = decoder.u64()?;
    let revision = decoder.u64()?;
    let owner = decode_owner(&mut decoder)?;
    let (location, motion_revision) = match decoder.u8()? {
        0 => {
            let checkpoint_motion = base.and_then(|record| match &record.location {
                EntityLocation::Mobile { position } => Some(EntityMotionSnapshot {
                    id,
                    revision: record.motion_revision,
                    position: *position,
                }),
                EntityLocation::Anchored { .. } => None,
            });
            let motion = select_motion(checkpoint_motion, wal_motion)?;
            (
                EntityLocation::Mobile {
                    position: motion.position,
                },
                motion.revision,
            )
        }
        1 => {
            if wal_motion.is_some() {
                return Err(EntityError::CorruptCheckpoint);
            }
            let anchor = decode_cell(&mut decoder)?;
            let anchor_state = BlockStateId(decoder.u32()?);
            let count = usize::from(decoder.u16()?);
            if count == 0 || count > MAX_ENTITY_FOOTPRINT_CELLS {
                return Err(EntityError::CorruptCheckpoint);
            }
            let mut footprint = Vec::with_capacity(count);
            let mut previous = None;
            for _ in 0..count {
                let cell = decode_cell(&mut decoder)?;
                if previous.is_some_and(|value| value >= cell) {
                    return Err(EntityError::CorruptCheckpoint);
                }
                previous = Some(cell);
                footprint.push(cell);
            }
            (
                EntityLocation::Anchored {
                    anchor,
                    anchor_state,
                    footprint,
                },
                0,
            )
        }
        _ => return Err(EntityError::CorruptCheckpoint),
    };
    let (next_tick, stored_payload) = decode_schedule_and_payload(&mut decoder)?;
    decoder.finish()?;
    let (descriptor, payload, payload_size, public_view) = decode_payload(
        entity_type,
        stored_schema_version,
        stored_schema_fingerprint,
        &stored_payload,
        types,
    )?;
    if owner != location.owner()? || revision == 0 {
        return Err(EntityError::CorruptCheckpoint);
    }
    Ok(EntityRecord {
        id,
        entity_type,
        schema_version: descriptor.schema_version(),
        schema_fingerprint: descriptor.schema_fingerprint(),
        owner,
        revision,
        motion_revision,
        location,
        payload,
        payload_size,
        public_view,
        next_tick,
    })
}

fn select_motion(
    checkpoint: Option<EntityMotionSnapshot>,
    wal: Option<EntityMotionSnapshot>,
) -> Result<EntityMotionSnapshot, EntityError> {
    match (checkpoint, wal) {
        (Some(checkpoint), Some(wal)) if checkpoint.revision == wal.revision => {
            if checkpoint.position.map(f32::to_bits) == wal.position.map(f32::to_bits) {
                Ok(checkpoint)
            } else {
                Err(EntityError::CorruptCheckpoint)
            }
        }
        (Some(checkpoint), Some(wal)) if checkpoint.revision > wal.revision => Ok(checkpoint),
        (_, Some(wal)) => Ok(wal),
        (Some(checkpoint), None) => Ok(checkpoint),
        (None, None) => Err(EntityError::CorruptCheckpoint),
    }
}

fn encode_schedule_and_payload(
    encoder: &mut Encoder,
    record: &EntityRecord,
    types: &EntityTypeRegistry,
) -> Result<(), EntityError> {
    match record.next_tick {
        Some(next_tick) => {
            encoder.u8(1);
            encoder.u64(next_tick);
        }
        None => encoder.u8(0),
    }
    let descriptor = types.descriptor(record.entity_type)?;
    let payload = descriptor.encode_payload(&record.payload)?;
    if payload.len() != record.payload_size {
        return Err(EntityError::InvalidPayload);
    }
    encoder.length_bytes(&payload)
}

fn decode_schedule_and_payload(
    decoder: &mut Decoder<'_>,
) -> Result<(Option<u64>, Vec<u8>), EntityError> {
    let next_tick = match decoder.u8()? {
        0 => None,
        1 => Some(decoder.u64()?),
        _ => return Err(EntityError::CorruptCheckpoint),
    };
    let payload = decoder.length_bytes(MAX_ENTITY_PAYLOAD_BYTES)?.to_vec();
    Ok((next_tick, payload))
}

fn decode_payload<'a>(
    entity_type: EntityTypeId,
    stored_schema_version: u16,
    stored_schema_fingerprint: u64,
    stored_payload: &[u8],
    types: &'a EntityTypeRegistry,
) -> Result<(&'a EntityTypeDescriptor, EntityPayload, usize, Vec<u8>), EntityError> {
    let descriptor = types
        .descriptor(entity_type)
        .map_err(|_| EntityError::UnknownRequiredType(entity_type))?;
    if stored_schema_version == 0
        || stored_schema_version > descriptor.schema_version()
        || (stored_schema_version == descriptor.schema_version()
            && stored_schema_fingerprint != descriptor.schema_fingerprint())
    {
        return Err(EntityError::CodecRejected);
    }
    let payload = descriptor.decode_payload(stored_schema_version, stored_payload)?;
    let payload_size = descriptor.encode_payload(&payload)?.len();
    let public_view = descriptor.public_view(&payload)?;
    Ok((descriptor, payload, payload_size, public_view))
}

fn decode_record_value_bytes(
    bytes: &[u8],
    types: &EntityTypeRegistry,
) -> Result<EntityRecord, EntityError> {
    let body = checked_body(bytes, MAX_ENTITY_RECORD_VALUE_BYTES)?;
    let mut decoder = Decoder::new(body);
    if decoder.raw(4)? != ENTITY_RECORD_MAGIC || decoder.u16()? != ENTITY_RECORD_VERSION {
        return Err(EntityError::CorruptCheckpoint);
    }
    let id = EntityId::new(decoder.u64()?).ok_or(EntityError::CorruptCheckpoint)?;
    let entity_type = EntityTypeId(decoder.u32()?);
    let stored_schema_version = decoder.u16()?;
    let stored_schema_fingerprint = decoder.u64()?;
    let revision = decoder.u64()?;
    let motion_revision = decoder.u64()?;
    let stored_owner = decode_owner(&mut decoder)?;
    let location = match decoder.u8()? {
        0 => EntityLocation::Mobile {
            position: [decoder.f32()?, decoder.f32()?, decoder.f32()?],
        },
        1 => {
            let anchor = decode_cell(&mut decoder)?;
            let anchor_state = BlockStateId(decoder.u32()?);
            let count = usize::from(decoder.u16()?);
            if count == 0 || count > MAX_ENTITY_FOOTPRINT_CELLS {
                return Err(EntityError::CorruptCheckpoint);
            }
            let mut footprint = Vec::with_capacity(count);
            let mut previous = None;
            for _ in 0..count {
                let cell = decode_cell(&mut decoder)?;
                if previous.is_some_and(|value| value >= cell) {
                    return Err(EntityError::CorruptCheckpoint);
                }
                previous = Some(cell);
                footprint.push(cell);
            }
            EntityLocation::Anchored {
                anchor,
                anchor_state,
                footprint,
            }
        }
        _ => return Err(EntityError::CorruptCheckpoint),
    };
    let next_tick = match decoder.u8()? {
        0 => None,
        1 => Some(decoder.u64()?),
        _ => return Err(EntityError::CorruptCheckpoint),
    };
    let stored_payload = decoder.length_bytes(MAX_ENTITY_PAYLOAD_BYTES)?.to_vec();
    decoder.finish()?;
    let descriptor = types
        .descriptor(entity_type)
        .map_err(|_| EntityError::UnknownRequiredType(entity_type))?;
    if stored_schema_version == 0
        || stored_schema_version > descriptor.schema_version()
        || (stored_schema_version == descriptor.schema_version()
            && stored_schema_fingerprint != descriptor.schema_fingerprint())
    {
        return Err(EntityError::CodecRejected);
    }
    let payload = descriptor.decode_payload(stored_schema_version, &stored_payload)?;
    let current_payload = descriptor.encode_payload(&payload)?;
    let payload_size = current_payload.len();
    let public_view = descriptor.public_view(&payload)?;
    let owner = location.owner()?;
    if owner != stored_owner || revision == 0 {
        return Err(EntityError::CorruptCheckpoint);
    }
    Ok(EntityRecord {
        id,
        entity_type,
        schema_version: descriptor.schema_version(),
        schema_fingerprint: descriptor.schema_fingerprint(),
        owner,
        revision,
        motion_revision,
        location,
        payload,
        payload_size,
        public_view,
        next_tick,
    })
}

pub fn encode_allocator_value(next_id: u64) -> Result<Vec<u8>, EntityError> {
    if next_id == 0 {
        return Err(EntityError::IdExhausted);
    }
    let mut encoder = Encoder::with_capacity(14);
    encoder.raw(ENTITY_ALLOCATOR_MAGIC);
    encoder.u16(ENTITY_ALLOCATOR_VERSION);
    encoder.u64(next_id);
    encoder.finish_crc()
}

pub fn decode_allocator_value(bytes: &[u8]) -> Result<u64, EntityError> {
    let body = checked_body(bytes, 32)?;
    let mut decoder = Decoder::new(body);
    if decoder.raw(4)? != ENTITY_ALLOCATOR_MAGIC || decoder.u16()? != ENTITY_ALLOCATOR_VERSION {
        return Err(EntityError::CorruptCheckpoint);
    }
    let next_id = decoder.u64()?;
    decoder.finish()?;
    if next_id == 0 {
        return Err(EntityError::CorruptCheckpoint);
    }
    Ok(next_id)
}

fn encode_owner(encoder: &mut Encoder, owner: EntityOwner) {
    let (tag, chunk) = match owner {
        EntityOwner::Mobile(chunk) => (0, chunk),
        EntityOwner::Anchored(chunk) => (1, chunk),
    };
    encoder.u8(tag);
    encoder.raw(&chunk.x.to_le_bytes());
    encoder.raw(&chunk.y.to_le_bytes());
    encoder.raw(&chunk.z.to_le_bytes());
}

fn decode_owner(decoder: &mut Decoder<'_>) -> Result<EntityOwner, EntityError> {
    let tag = decoder.u8()?;
    let chunk = ChunkKey {
        x: decoder.i32()?,
        y: decoder.i32()?,
        z: decoder.i32()?,
    };
    match tag {
        0 => Ok(EntityOwner::Mobile(chunk)),
        1 => Ok(EntityOwner::Anchored(chunk)),
        _ => Err(EntityError::CorruptCheckpoint),
    }
}

fn encode_cell(encoder: &mut Encoder, cell: CellCoord) {
    encoder.raw(&cell.x.to_le_bytes());
    encoder.raw(&cell.y.to_le_bytes());
    encoder.raw(&cell.z.to_le_bytes());
}

fn decode_cell(decoder: &mut Decoder<'_>) -> Result<CellCoord, EntityError> {
    Ok(CellCoord::new(
        decoder.i32()?,
        decoder.i32()?,
        decoder.i32()?,
    ))
}

fn enforce_size(encoder: &Encoder) -> Result<(), EntityError> {
    if encoder.len() > MAX_ENTITY_SNAPSHOT_BYTES.saturating_sub(4) {
        Err(EntityError::PayloadTooLarge)
    } else {
        Ok(())
    }
}

#[allow(dead_code)]
fn _entity_page_key_round_trip(chunk: ChunkKey) -> bool {
    decode_chunk_key(&encode_chunk_key(chunk)).is_ok_and(|decoded| decoded == chunk)
}

#[allow(dead_code)]
fn _entity_cell_key_round_trip(cell: CellCoord) -> bool {
    decode_cell_key(&encode_cell_key(cell)).is_ok_and(|decoded| decoded == cell)
}

#[allow(dead_code)]
fn _journal_domains_are_sorted(changes: &[StateKey]) -> bool {
    changes.windows(2).all(|pair| pair[0] < pair[1])
}

#[allow(dead_code)]
fn _ordered_cell_map(cells: &BTreeSet<CellCoord>) -> usize {
    cells.len()
}

use super::codec::{
    Decoder, ENTITY_CELL_VALUE_MAGIC, ENTITY_CELL_VALUE_VERSION, ENTITY_CHUNK_PAGE_MAGIC,
    ENTITY_CHUNK_PAGE_VERSION, Encoder,
};
use super::store::EntityRecord;
use super::types::{
    CellCoord, EntityError, EntityId, EntityLocation, EntityOwner, EntityOwnership,
    MAX_ENTITY_PRIVATE_BYTES_PER_CHUNK, MAX_ENTITY_PUBLIC_BYTES_PER_CHUNK,
    MAX_ENTITY_REFERENCES_PER_CHUNK, position_to_cell,
};
use crate::content::EntityTypeId;
use crate::world::ChunkKey;
use std::collections::{BTreeMap, BTreeSet};

const MOBILE_BUCKET_SIZE: f32 = 16.0;
const MAX_BUCKETS_PER_QUERY: usize = 4_096;
const MAX_QUERY_CANDIDATES: usize = 65_536;

/// Exact before/after bytes for affected chunk pages and anchored cells.
pub type EntityIndexChanges = (
    Vec<(ChunkKey, Vec<u8>, Vec<u8>)>,
    Vec<(CellCoord, Vec<u8>, Vec<u8>)>,
);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Bucket(i32, i32, i32);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChunkPage {
    pub entity_ids: BTreeSet<EntityId>,
    pub private_bytes: usize,
    pub public_bytes: usize,
}

impl ChunkPage {
    pub fn add(
        &mut self,
        id: EntityId,
        private_bytes: usize,
        public_bytes: usize,
        chunk: ChunkKey,
    ) -> Result<(), EntityError> {
        if self.entity_ids.contains(&id) {
            return Err(EntityError::InvalidTransaction);
        }
        if self.entity_ids.len() >= MAX_ENTITY_REFERENCES_PER_CHUNK {
            return Err(EntityError::ChunkReferenceBudgetExceeded(chunk));
        }
        let private_bytes = self
            .private_bytes
            .checked_add(private_bytes)
            .ok_or(EntityError::ChunkPayloadBudgetExceeded(chunk))?;
        let public_bytes = self
            .public_bytes
            .checked_add(public_bytes)
            .ok_or(EntityError::ChunkPublicViewBudgetExceeded(chunk))?;
        if private_bytes > MAX_ENTITY_PRIVATE_BYTES_PER_CHUNK {
            return Err(EntityError::ChunkPayloadBudgetExceeded(chunk));
        }
        if public_bytes > MAX_ENTITY_PUBLIC_BYTES_PER_CHUNK {
            return Err(EntityError::ChunkPublicViewBudgetExceeded(chunk));
        }
        self.private_bytes = private_bytes;
        self.public_bytes = public_bytes;
        self.entity_ids.insert(id);
        Ok(())
    }

    pub fn remove(
        &mut self,
        id: EntityId,
        private_bytes: usize,
        public_bytes: usize,
    ) -> Result<(), EntityError> {
        if !self.entity_ids.remove(&id) {
            return Err(EntityError::InvalidTransaction);
        }
        self.private_bytes = self
            .private_bytes
            .checked_sub(private_bytes)
            .ok_or(EntityError::InvalidTransaction)?;
        self.public_bytes = self
            .public_bytes
            .checked_sub(public_bytes)
            .ok_or(EntityError::InvalidTransaction)?;
        Ok(())
    }

    pub fn encode_value(&self) -> Result<Vec<u8>, EntityError> {
        if self.entity_ids.is_empty() {
            if self.private_bytes == 0 && self.public_bytes == 0 {
                return Ok(Vec::new());
            }
            return Err(EntityError::InvalidTransaction);
        }
        let mut encoder = Encoder::with_capacity(18 + self.entity_ids.len() * 8);
        encoder.raw(ENTITY_CHUNK_PAGE_MAGIC);
        encoder.u16(ENTITY_CHUNK_PAGE_VERSION);
        encoder
            .u32(u32::try_from(self.entity_ids.len()).map_err(|_| EntityError::TooManyEntities)?);
        encoder.u32(u32::try_from(self.private_bytes).map_err(|_| EntityError::PayloadTooLarge)?);
        encoder.u32(u32::try_from(self.public_bytes).map_err(|_| EntityError::PublicViewTooLarge)?);
        for id in &self.entity_ids {
            encoder.u64(id.get());
        }
        encoder.finish_crc()
    }

    pub fn decode_value(bytes: &[u8]) -> Result<Self, EntityError> {
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        let body = super::codec::checked_body(bytes, 1_048_576)?;
        let mut decoder = Decoder::new(body);
        if decoder.raw(4)? != ENTITY_CHUNK_PAGE_MAGIC || decoder.u16()? != ENTITY_CHUNK_PAGE_VERSION
        {
            return Err(EntityError::CorruptCheckpoint);
        }
        let count = usize::try_from(decoder.u32()?).map_err(|_| EntityError::CorruptCheckpoint)?;
        if count == 0 || count > MAX_ENTITY_REFERENCES_PER_CHUNK {
            return Err(EntityError::CorruptCheckpoint);
        }
        let private_bytes =
            usize::try_from(decoder.u32()?).map_err(|_| EntityError::CorruptCheckpoint)?;
        let public_bytes =
            usize::try_from(decoder.u32()?).map_err(|_| EntityError::CorruptCheckpoint)?;
        if private_bytes > MAX_ENTITY_PRIVATE_BYTES_PER_CHUNK
            || public_bytes > MAX_ENTITY_PUBLIC_BYTES_PER_CHUNK
        {
            return Err(EntityError::CorruptCheckpoint);
        }
        let mut entity_ids = BTreeSet::new();
        let mut previous = None;
        for _ in 0..count {
            let id = EntityId::new(decoder.u64()?).ok_or(EntityError::CorruptCheckpoint)?;
            if previous.is_some_and(|value| value >= id) {
                return Err(EntityError::CorruptCheckpoint);
            }
            previous = Some(id);
            entity_ids.insert(id);
        }
        decoder.finish()?;
        Ok(Self {
            entity_ids,
            private_bytes,
            public_bytes,
        })
    }
}

#[derive(Clone, Default)]
pub struct EntityIndexes {
    pub chunks: BTreeMap<ChunkKey, ChunkPage>,
    pub anchored_cells: BTreeMap<CellCoord, EntityId>,
    pub schedule: BTreeMap<u64, BTreeSet<EntityId>>,
    tick_schedule: BTreeSet<(u64, EntityId)>,
    suspended_ticks: BTreeSet<EntityId>,
    tick_types: BTreeSet<EntityTypeId>,
    mobile: MobileSpatialIndex,
}

impl EntityIndexes {
    pub fn with_tick_types(tick_types: impl IntoIterator<Item = EntityTypeId>) -> Self {
        Self {
            tick_types: tick_types.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn preview_change(
        &self,
        before: Option<&EntityRecord>,
        after: Option<&EntityRecord>,
    ) -> Result<EntityIndexChanges, EntityError> {
        if before.map(|record| record.id) != after.map(|record| record.id)
            && before.is_some()
            && after.is_some()
        {
            return Err(EntityError::InvalidTransaction);
        }
        let id = before
            .map(|record| record.id)
            .or_else(|| after.map(|record| record.id))
            .ok_or(EntityError::NoChanges)?;
        let mut touched_chunks = BTreeSet::new();
        let mut touched_cells = BTreeSet::new();
        for record in before.into_iter().chain(after) {
            touched_chunks.extend(record.location.touched_chunks()?);
            if let EntityLocation::Anchored { footprint, .. } = &record.location {
                touched_cells.extend(footprint.iter().copied());
            }
        }

        let mut changed_pages = Vec::new();
        for chunk in touched_chunks {
            let before_bytes = self
                .chunks
                .get(&chunk)
                .cloned()
                .unwrap_or_default()
                .encode_value()?;
            let mut page = self.chunks.get(&chunk).cloned().unwrap_or_default();
            if let Some(record) = before.filter(|record| {
                record
                    .location
                    .touched_chunks()
                    .is_ok_and(|chunks| chunks.contains(&chunk))
            }) {
                let private_bytes = if record.owner.chunk() == chunk {
                    record.payload_size
                } else {
                    0
                };
                page.remove(record.id, private_bytes, record.public_view.len())?;
            }
            if let Some(record) = after.filter(|record| {
                record
                    .location
                    .touched_chunks()
                    .is_ok_and(|chunks| chunks.contains(&chunk))
            }) {
                let private_bytes = if record.owner.chunk() == chunk {
                    record.payload_size
                } else {
                    0
                };
                page.add(record.id, private_bytes, record.public_view.len(), chunk)?;
            }
            let after_bytes = page.encode_value()?;
            if before_bytes != after_bytes {
                changed_pages.push((chunk, before_bytes, after_bytes));
            }
        }

        let mut changed_cells = Vec::new();
        for cell in touched_cells {
            let before_owner = self.anchored_cells.get(&cell).copied();
            if before_owner.is_some_and(|owner| owner != id) {
                return Err(EntityError::FootprintOverlap(cell));
            }
            let in_before = before.is_some_and(|record| {
                matches!(&record.location, EntityLocation::Anchored { footprint, .. } if footprint.contains(&cell))
            });
            let in_after = after.is_some_and(|record| {
                matches!(&record.location, EntityLocation::Anchored { footprint, .. } if footprint.contains(&cell))
            });
            let expected_before = if in_before { Some(id) } else { None };
            if before_owner != expected_before {
                return Err(EntityError::InvalidTransaction);
            }
            if in_before != in_after {
                let before_bytes = encode_cell_owner(before_owner)?;
                let after_bytes = encode_cell_owner(in_after.then_some(id))?;
                changed_cells.push((cell, before_bytes, after_bytes));
            }
        }
        Ok((changed_pages, changed_cells))
    }

    pub fn replace(
        &mut self,
        before: Option<&EntityRecord>,
        after: Option<&EntityRecord>,
    ) -> Result<(), EntityError> {
        if let Some(record) = before {
            self.remove(record)?;
        }
        if let Some(record) = after {
            self.insert(record)?;
        }
        Ok(())
    }

    pub fn validate_cell_owner(&self, cell: CellCoord, id: EntityId) -> Result<(), EntityError> {
        if self
            .anchored_cells
            .get(&cell)
            .is_some_and(|owner| *owner != id)
        {
            Err(EntityError::FootprintOverlap(cell))
        } else {
            Ok(())
        }
    }

    pub fn insert(&mut self, record: &EntityRecord) -> Result<(), EntityError> {
        for chunk in record.location.touched_chunks()? {
            let page = self.chunks.entry(chunk).or_default();
            let private_bytes = if record.owner.chunk() == chunk {
                record.payload_size
            } else {
                0
            };
            if let Err(error) = page.add(record.id, private_bytes, record.public_view.len(), chunk)
            {
                if page.entity_ids.is_empty() {
                    self.chunks.remove(&chunk);
                }
                return Err(error);
            }
        }
        if let EntityLocation::Anchored { footprint, .. } = &record.location {
            for cell in footprint {
                self.validate_cell_owner(*cell, record.id)?;
            }
            for cell in footprint {
                self.anchored_cells.insert(*cell, record.id);
            }
        } else if let EntityLocation::Mobile { position } = &record.location {
            self.mobile.insert(record.id, *position)?;
        }
        if let Some(next_tick) = record.next_tick {
            self.schedule
                .entry(next_tick)
                .or_default()
                .insert(record.id);
            if self.tick_types.contains(&record.entity_type) {
                self.tick_schedule.insert((next_tick, record.id));
            }
        } else if self.tick_types.contains(&record.entity_type) {
            self.suspended_ticks.insert(record.id);
        }
        Ok(())
    }

    pub fn remove(&mut self, record: &EntityRecord) -> Result<(), EntityError> {
        for chunk in record.location.touched_chunks()? {
            let page = self
                .chunks
                .get_mut(&chunk)
                .ok_or(EntityError::InvalidTransaction)?;
            let private_bytes = if record.owner.chunk() == chunk {
                record.payload_size
            } else {
                0
            };
            page.remove(record.id, private_bytes, record.public_view.len())?;
            if page.entity_ids.is_empty() {
                self.chunks.remove(&chunk);
            }
        }
        match &record.location {
            EntityLocation::Anchored { footprint, .. } => {
                for cell in footprint {
                    if self.anchored_cells.remove(cell) != Some(record.id) {
                        return Err(EntityError::InvalidTransaction);
                    }
                }
            }
            EntityLocation::Mobile { .. } => self.mobile.remove(record.id),
        }
        if let Some(next_tick) = record.next_tick
            && let Some(ids) = self.schedule.get_mut(&next_tick)
        {
            ids.remove(&record.id);
            if ids.is_empty() {
                self.schedule.remove(&next_tick);
            }
        }
        if let Some(next_tick) = record.next_tick {
            self.tick_schedule.remove(&(next_tick, record.id));
        }
        self.suspended_ticks.remove(&record.id);
        Ok(())
    }

    /// One circular recheck candidate and this pass's fixed upper ID. New
    /// spawns cannot extend a pass forever and starve earlier IDs. Unavailable
    /// candidates advance the cursor too; deletion needs no cursor repair.
    pub fn suspended_tick_after(
        &self,
        after: Option<(EntityId, EntityId)>,
    ) -> Option<(EntityId, EntityId)> {
        use std::ops::Bound::{Excluded, Included};
        if let Some((id, through)) = after
            && let Some(next) = self
                .suspended_ticks
                .range((Excluded(id), Included(through)))
                .next()
        {
            return Some((*next, through));
        }
        Some((
            *self.suspended_ticks.first()?,
            *self.suspended_ticks.last()?,
        ))
    }

    /// Read a bounded due slice after `after`, wrapping to the first due key
    /// when the cursor reaches the end. The caller advances its cursor only
    /// through entries it actually considered for admission.
    pub fn due_tick_entries(
        &self,
        through_tick: u64,
        after: Option<(u64, EntityId)>,
        maximum: usize,
    ) -> Vec<(u64, EntityId)> {
        use std::ops::Bound::{Excluded, Unbounded};

        if maximum == 0 {
            return Vec::new();
        }
        let after_cursor: Box<dyn Iterator<Item = &(u64, EntityId)> + '_> = match after {
            Some(cursor) => Box::new(self.tick_schedule.range((Excluded(cursor), Unbounded))),
            None => Box::new(self.tick_schedule.iter()),
        };
        let mut entries: Vec<_> = after_cursor
            .take_while(|entry| entry.0 <= through_tick)
            .take(maximum)
            .copied()
            .collect();
        if entries.len() < maximum {
            let remaining = maximum - entries.len();
            entries.extend(
                self.tick_schedule
                    .iter()
                    .take_while(|entry| entry.0 <= through_tick)
                    .filter(|entry| after.is_none_or(|cursor| **entry <= cursor))
                    .take(remaining)
                    .copied(),
            );
        }
        entries
    }

    pub fn due(&self, through_tick: u64, maximum: usize) -> Vec<EntityId> {
        self.schedule
            .range(..=through_tick)
            .flat_map(|(_, ids)| ids.iter().copied())
            .take(maximum)
            .collect()
    }

    pub fn mobile_query(&self, min: [f32; 3], max: [f32; 3]) -> Result<Vec<EntityId>, EntityError> {
        self.mobile.query(min, max)
    }

    pub fn validate_against_records(
        &self,
        records: &BTreeMap<EntityId, EntityRecord>,
    ) -> Result<(), EntityError> {
        let mut rebuilt = Self::with_tick_types(self.tick_types.iter().copied());
        for record in records.values() {
            rebuilt.insert(record)?;
        }
        if rebuilt.chunks != self.chunks
            || rebuilt.anchored_cells != self.anchored_cells
            || rebuilt.schedule != self.schedule
            || rebuilt.tick_schedule != self.tick_schedule
            || rebuilt.suspended_ticks != self.suspended_ticks
            || rebuilt.tick_types != self.tick_types
            || rebuilt.mobile != self.mobile
        {
            return Err(EntityError::InvalidTransaction);
        }
        Ok(())
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct MobileSpatialIndex {
    buckets: BTreeMap<Bucket, BTreeSet<EntityId>>,
    by_id: BTreeMap<EntityId, Bucket>,
}

impl MobileSpatialIndex {
    fn bucket(position: [f32; 3]) -> Result<Bucket, EntityError> {
        position_to_cell(position)?;
        let axis = |value: f32| (value / MOBILE_BUCKET_SIZE).floor() as i32;
        Ok(Bucket(
            axis(position[0]),
            axis(position[1]),
            axis(position[2]),
        ))
    }

    fn insert(&mut self, id: EntityId, position: [f32; 3]) -> Result<(), EntityError> {
        let bucket = Self::bucket(position)?;
        if self.by_id.contains_key(&id) {
            return Err(EntityError::InvalidTransaction);
        }
        self.buckets.entry(bucket).or_default().insert(id);
        self.by_id.insert(id, bucket);
        Ok(())
    }

    fn remove(&mut self, id: EntityId) {
        let Some(bucket) = self.by_id.remove(&id) else {
            return;
        };
        if let Some(ids) = self.buckets.get_mut(&bucket) {
            ids.remove(&id);
            if ids.is_empty() {
                self.buckets.remove(&bucket);
            }
        }
    }

    fn query(&self, min: [f32; 3], max: [f32; 3]) -> Result<Vec<EntityId>, EntityError> {
        if min.iter().chain(max.iter()).any(|value| !value.is_finite())
            || (0..3).any(|axis| min[axis] > max[axis])
        {
            return Err(EntityError::InvalidLocation);
        }
        let min = Self::bucket(min)?;
        let max = Self::bucket(max)?;
        let widths = [
            i64::from(max.0) - i64::from(min.0) + 1,
            i64::from(max.1) - i64::from(min.1) + 1,
            i64::from(max.2) - i64::from(min.2) + 1,
        ];
        let bucket_count = widths.into_iter().try_fold(1u64, |product, width| {
            product.checked_mul(u64::try_from(width).ok()?)
        });
        if bucket_count.is_none_or(|count| count > MAX_BUCKETS_PER_QUERY as u64) {
            return Err(EntityError::SpatialQueryTooBroad);
        }
        let mut ids = BTreeSet::new();
        for x in i64::from(min.0)..=i64::from(max.0) {
            for y in i64::from(min.1)..=i64::from(max.1) {
                for z in i64::from(min.2)..=i64::from(max.2) {
                    let bucket = Bucket(x as i32, y as i32, z as i32);
                    if let Some(bucket_ids) = self.buckets.get(&bucket) {
                        ids.extend(bucket_ids.iter().copied());
                        if ids.len() > MAX_QUERY_CANDIDATES {
                            return Err(EntityError::SpatialQueryTooBroad);
                        }
                    }
                }
            }
        }
        Ok(ids.into_iter().collect())
    }
}

pub fn encode_chunk_key(chunk: ChunkKey) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(12);
    bytes.extend(chunk.x.to_le_bytes());
    bytes.extend(chunk.y.to_le_bytes());
    bytes.extend(chunk.z.to_le_bytes());
    bytes
}

pub fn decode_chunk_key(bytes: &[u8]) -> Result<ChunkKey, EntityError> {
    if bytes.len() != 12 {
        return Err(EntityError::CorruptCheckpoint);
    }
    let mut decoder = Decoder::new(bytes);
    let key = ChunkKey {
        x: decoder.i32()?,
        y: decoder.i32()?,
        z: decoder.i32()?,
    };
    decoder.finish()?;
    Ok(key)
}

pub fn encode_cell_key(cell: CellCoord) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(12);
    bytes.extend(cell.x.to_le_bytes());
    bytes.extend(cell.y.to_le_bytes());
    bytes.extend(cell.z.to_le_bytes());
    bytes
}

pub fn decode_cell_key(bytes: &[u8]) -> Result<CellCoord, EntityError> {
    if bytes.len() != 12 {
        return Err(EntityError::CorruptCheckpoint);
    }
    let mut decoder = Decoder::new(bytes);
    let cell = CellCoord::new(decoder.i32()?, decoder.i32()?, decoder.i32()?);
    decoder.finish()?;
    Ok(cell)
}

pub fn encode_cell_owner(id: Option<EntityId>) -> Result<Vec<u8>, EntityError> {
    let Some(id) = id else {
        return Ok(Vec::new());
    };
    let mut encoder = Encoder::with_capacity(14);
    encoder.raw(ENTITY_CELL_VALUE_MAGIC);
    encoder.u16(ENTITY_CELL_VALUE_VERSION);
    encoder.u64(id.get());
    encoder.finish_crc()
}

pub fn decode_cell_owner(bytes: &[u8]) -> Result<Option<EntityId>, EntityError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let body = super::codec::checked_body(bytes, 32)?;
    let mut decoder = Decoder::new(body);
    if decoder.raw(4)? != ENTITY_CELL_VALUE_MAGIC || decoder.u16()? != ENTITY_CELL_VALUE_VERSION {
        return Err(EntityError::CorruptCheckpoint);
    }
    let id = EntityId::new(decoder.u64()?).ok_or(EntityError::CorruptCheckpoint)?;
    decoder.finish()?;
    Ok(Some(id))
}

pub fn validate_location_owner(
    location: &EntityLocation,
    owner: EntityOwner,
) -> Result<(), EntityError> {
    if location.owner()? != owner {
        Err(EntityError::InvalidLocation)
    } else {
        Ok(())
    }
}

pub fn validate_ownership_mode(
    location: &EntityLocation,
    ownership: &EntityOwnership,
) -> Result<(), EntityError> {
    match (location, ownership) {
        (EntityLocation::Mobile { .. }, EntityOwnership::Mobile) => Ok(()),
        (
            EntityLocation::Anchored {
                anchor,
                anchor_state,
                footprint,
            },
            EntityOwnership::Anchored {
                compatible_anchor_states,
                max_footprint_cells,
            },
        ) => {
            if !compatible_anchor_states.contains(anchor_state) {
                return Err(EntityError::IncompatibleAnchorState(*anchor_state));
            }
            if footprint.is_empty()
                || footprint.len() > *max_footprint_cells
                || footprint.len() > super::types::MAX_ENTITY_FOOTPRINT_CELLS
                || !footprint.windows(2).all(|pair| pair[0] < pair[1])
                || footprint.binary_search(anchor).is_err()
            {
                return Err(EntityError::InvalidLocation);
            }
            Ok(())
        }
        _ => Err(EntityError::WrongOwnership),
    }
}

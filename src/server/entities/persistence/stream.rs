//! Immutable, worker-owned BGEN traversal. The rotation fence holds the mirror
//! fixed until publication finishes; iterators borrow it, never clone records,
//! indexes, or publication pages. Each part is one bounded schema value.

use super::*;
use crate::server::checkpoint_stream;
use std::io::{self, Write};

pub(in crate::server) fn write_checkpoint(
    store: &EntityStore,
    output: &mut impl Write,
    quota: usize,
    after_turn: impl FnMut(usize) -> io::Result<()>,
) -> io::Result<()> {
    let invalid = |error| io::Error::new(io::ErrorKind::InvalidData, error);
    if store.len() > MAX_ENTITY_RECORDS
        || store.chunk_pages().len() > MAX_CHECKPOINT_PAGES
        || store.indexes().anchored_cells.len() > MAX_CHECKPOINT_CELLS
    {
        return Err(invalid(EntityError::TooManyEntities));
    }
    let mut header = Encoder::with_capacity(50);
    header.raw(CHECKPOINT_MAGIC);
    header.u16(CHECKPOINT_VERSION);
    header.u64(store.next_id());
    header.u64(store.revision());
    header.u64(store.durable_sequence());
    header.u64(store.durable_global_revision());
    header.u32(store.len() as u32);
    header.u32(store.chunk_pages().len() as u32);
    header.u32(store.indexes().anchored_cells.len() as u32);
    let records = store.record_values().map(|record| {
        if record.payload_size > MAX_ENTITY_PAYLOAD_BYTES
            || matches!(&record.location, EntityLocation::Anchored { footprint, .. }
                if footprint.len() > MAX_ENTITY_FOOTPRINT_CELLS)
        {
            return Err(invalid(EntityError::PayloadTooLarge));
        }
        let value = encode_record_value(record, store.types()).map_err(invalid)?;
        if value.len() > MAX_ENTITY_RECORD_VALUE_BYTES {
            return Err(invalid(EntityError::PayloadTooLarge));
        }
        let mut part = Encoder::with_capacity(value.len() + 4);
        part.length_bytes(&value).map_err(invalid)?;
        Ok(part.into_bytes())
    });
    let pages = store.chunk_pages().iter().map(|(chunk, page)| {
        // Check before page encoding/allocation, not after collecting the IDs.
        if page.entity_ids.len() > super::super::types::MAX_ENTITY_REFERENCES_PER_CHUNK {
            return Err(invalid(EntityError::TooManyEntities));
        }
        let mut part = Encoder::with_capacity(16);
        part.i32(chunk.x);
        part.i32(chunk.y);
        part.i32(chunk.z);
        part.length_bytes(&page.encode_value().map_err(invalid)?)
            .map_err(invalid)?;
        Ok(part.into_bytes())
    });
    let cells = store.indexes().anchored_cells.iter().map(|(cell, id)| {
        let mut part = Encoder::with_capacity(48);
        part.i32(cell.x);
        part.i32(cell.y);
        part.i32(cell.z);
        part.length_bytes(&encode_cell_owner(Some(*id)).map_err(invalid)?)
            .map_err(invalid)?;
        Ok(part.into_bytes())
    });
    checkpoint_stream::write_frame(
        output,
        MAX_ENTITY_SNAPSHOT_BYTES,
        std::iter::once(Ok(header.into_bytes()))
            .chain(records)
            .chain(pages)
            .chain(cells),
        quota,
        after_turn,
    )
}

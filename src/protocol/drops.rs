//! Catalog-validated, bounded stack presentation for world drops and pickups.
use super::{Catalog, Cursor, DroppedItem, MAX_FRAME, invalid};
use crate::inventory::{MAX_COMPONENT_BYTES, Stack};
use crate::items::ItemId;
use std::io;

const MAX_ITEMS: usize = 256;
const FIXED_ITEM_BYTES: usize = 8 + 4 + 2 + 12 + 4 + 2 + 2;

impl DroppedItem {
    pub(crate) fn stack(&self) -> Stack {
        Stack {
            item: self.item,
            count: self.count,
            components: self.components.clone(),
        }
    }

    fn wire_len(&self) -> usize {
        FIXED_ITEM_BYTES + self.components.as_ref().map_or(0, |p| p.bytes.len())
    }
}

pub(super) fn items_wire_len(items: &[DroppedItem]) -> usize {
    items.iter().map(DroppedItem::wire_len).sum()
}

fn prefix_count(items: &[DroppedItem], budget: usize) -> usize {
    let mut bytes = 0;
    items
        .iter()
        .take(MAX_ITEMS)
        .take_while(|item| {
            bytes += item.wire_len();
            bytes <= budget
        })
        .count()
}

/// One whole nearest-first snapshot, including version/tag, revision and count.
pub(crate) fn snapshot_count(items: &[DroppedItem]) -> usize {
    prefix_count(items, MAX_FRAME - 2 - 8 - 2)
}

/// Reliable pickup notifications keep every exact stack, in bounded frames.
pub(crate) fn pickup_pages(mut items: &[DroppedItem]) -> impl Iterator<Item = &[DroppedItem]> {
    std::iter::from_fn(move || {
        if items.is_empty() {
            return None;
        }
        let count = prefix_count(items, MAX_FRAME - 2 - 2);
        // Authoritative stacks have at most 1 KiB components, so one always fits.
        assert!(count > 0, "invalid oversized authoritative pickup");
        let (page, rest) = items.split_at(count);
        items = rest;
        Some(page)
    })
}

pub(super) fn write_items(
    out: &mut Vec<u8>,
    items: &[DroppedItem],
    catalog: &Catalog,
) -> io::Result<()> {
    if items.len() > MAX_ITEMS {
        return Err(invalid("too many drops"));
    }
    out.extend((items.len() as u16).to_le_bytes());
    for item in items {
        if item.id == 0
            || !item.stack().valid_in(catalog)
            || item.position.iter().any(|n| !n.is_finite())
        {
            return Err(invalid("invalid dropped stack"));
        }
        out.extend(item.id.to_le_bytes());
        out.extend(item.item.0.to_le_bytes());
        out.extend(item.count.to_le_bytes());
        for n in item.position {
            out.extend(n.to_le_bytes());
        }
        out.extend(item.age_ms.to_le_bytes());
        if let Some(payload) = &item.components {
            out.extend(payload.version.to_le_bytes());
            out.extend((payload.bytes.len() as u16).to_le_bytes());
            out.extend(&payload.bytes);
        } else {
            out.extend([0; 4]);
        }
    }
    Ok(())
}

pub(super) fn read_items(c: &mut Cursor<'_>, catalog: &Catalog) -> io::Result<Vec<DroppedItem>> {
    let count = usize::from(c.u16()?);
    if count > MAX_ITEMS {
        return Err(invalid("too many drops"));
    }
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let id = c.u64()?;
        let item = ItemId(c.u32()?);
        let count = c.u16()?;
        let position = [c.f32()?, c.f32()?, c.f32()?];
        let age_ms = c.u32()?;
        let version = c.u16()?;
        let len = usize::from(c.u16()?);
        if len > MAX_COMPONENT_BYTES || (len == 0 && version != 0) {
            return Err(invalid("invalid dropped components"));
        }
        let stack = if len == 0 {
            Stack::new(item, count)
        } else {
            Stack::with_components(item, count, version, c.take(len)?.to_vec())
                .ok_or_else(|| invalid("invalid dropped components"))?
        };
        if id == 0 || !stack.valid_in(catalog) {
            return Err(invalid("invalid dropped stack"));
        }
        items.push(DroppedItem {
            id,
            item,
            count,
            components: stack.components,
            position,
            age_ms,
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests;

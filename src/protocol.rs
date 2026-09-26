//! Small, versioned, length-prefixed wire format shared by the client and server.
use crate::content::{BlockStateId, Catalog, MAX_MANIFEST_BYTES};
use crate::inventory::{MAX_COMPONENT_BYTES, SLOTS, STACK_LIMIT, Stack};
use crate::items::ItemId;
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey, PaletteView, PalettedBlocks};
use std::io::{self, Read, Write};

mod entities;
pub use entities::{
    BlockCellChange, EntitySnapshotPage, PublicEntity, PublicEntityChange, PublicEntityLocation,
    WorldCommitPart, WorldSnapshotStart, snapshot_checksum,
};
pub use entities::{MAX_ENTITY_SNAPSHOT_PAGES, MAX_WORLD_COMMIT_BYTES, MAX_WORLD_COMMIT_PARTS};

pub const MAX_FRAME: usize = 64 * 1024;
pub const MAX_MANIFEST_PART: usize = 60 * 1024;
pub const MAX_ENTITY_INTERACT_BYTES: usize = 256;
const WIRE_VERSION: u8 = 8;
pub const MIN_VIEW_DISTANCE: u8 = 1;
pub const MAX_VIEW_DISTANCE: u8 = 6;
const MAX_NAME: usize = 32;
const BLOCK_COUNT: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

fn valid_action_id(id: u128) -> bool {
    (id >> 64) != 0 && (id as u64) != 0
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientMessage {
    Hello {
        name: String,
        profile: u128,
        content_fingerprint: u64,
    },
    Move {
        seq: u64,
        dx: f32,
        dy: f32,
        dz: f32,
    },
    Edit {
        action_id: u128,
        x: i32,
        y: i32,
        z: i32,
        block: BlockStateId,
        slot: u8,
    },
    InventoryMove {
        action_id: u128,
        from: u8,
        to: u8,
        count: u16,
    },
    DropStack {
        action_id: u128,
        slot: u8,
        count: u16,
    },
    /// Server-authorized creative grant, journaled with an action receipt.
    AdminGive {
        action_id: u128,
        item: ItemId,
        count: u16,
    },
    /// Opaque, bounded command for the entity anchored at a touched world cell.
    /// The server resolves type, reach, and private inventory authority.
    EntityInteract {
        action_id: u128,
        target: [i32; 3],
        payload: Vec<u8>,
    },
    Resync {
        key: ChunkKey,
    },
    SetView {
        radius: u8,
    },
    Ping {
        nonce: u64,
    },
    ContentReady {
        fingerprint: u64,
    },
    ActionAck {
        epoch: u64,
        through_seq: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DroppedItem {
    pub id: u64,
    pub item: ItemId,
    pub count: u16,
    pub position: [f32; 3],
    /// Age at snapshot time; enough range for a stable hover phase until expiry.
    pub age_ms: u32,
}

#[derive(Debug, Clone)]
pub enum ServerMessage {
    Welcome {
        id: u64,
        seed: u64,
    },
    Position {
        ack_seq: u64,
        x: f32,
        y: f32,
        z: f32,
    },
    Chunk(Chunk),
    Delta {
        key: ChunkKey,
        version: u64,
        x: u8,
        y: u8,
        z: u8,
        block: BlockStateId,
    },
    EditRejected {
        reason: String,
    },
    ActionResult {
        action_id: u128,
        accepted: bool,
        reason: String,
    },
    ActionSession {
        epoch: u64,
        next_seq: u64,
        acked_seq: u64,
    },
    ActionDeferred {
        action_id: u128,
    },
    ViewDistance {
        radius: u8,
    },
    Inventory {
        revision: u64,
        slots: [Option<Stack>; SLOTS],
    },
    Drops {
        revision: u64,
        items: Vec<DroppedItem>,
    },
    Pickups {
        items: Vec<DroppedItem>,
    },
    Pong {
        nonce: u64,
    },
    ContentManifestPart {
        fingerprint: u64,
        total_len: u32,
        offset: u32,
        bytes: Vec<u8>,
    },
    WorldSnapshotStart(WorldSnapshotStart),
    EntitySnapshotPage(EntitySnapshotPage),
    WorldCommitPart(WorldCommitPart),
    OwnedEntity {
        id: u64,
    },
}

/// Exact frame length for a valid server message, including its four-byte
/// length prefix. Outbound telemetry uses this without serializing a chunk a
/// second time on the simulation thread.
pub(crate) fn server_wire_len(message: &ServerMessage) -> usize {
    const HEADER: usize = 4 + 2; // length, wire version, message tag
    const DROP_ITEM: usize = 8 + 4 + 2 + 12 + 4;
    HEADER
        + match message {
            ServerMessage::Welcome { .. } => 8 + 8,
            ServerMessage::Position { .. } => 8 + 12,
            ServerMessage::Chunk(chunk) => {
                let unique = chunk.blocks.unique_states();
                12 + 8 + 2 + 1 + unique * 4 + BLOCK_COUNT * if unique <= 256 { 1 } else { 2 }
            }
            ServerMessage::Delta { .. } => 12 + 8 + 3 + 4,
            ServerMessage::EditRejected { reason } => 1 + reason.len(),
            ServerMessage::ActionResult { reason, .. } => 16 + 1 + 1 + reason.len(),
            ServerMessage::ActionSession { .. } => 8 + 8 + 8,
            ServerMessage::ActionDeferred { .. } => 16,
            ServerMessage::Pong { .. } => 8,
            ServerMessage::ContentManifestPart { bytes, .. } => 8 + 4 + 4 + 2 + bytes.len(),
            ServerMessage::ViewDistance { .. } => 1,
            ServerMessage::Inventory { slots, .. } => {
                8 + slots
                    .iter()
                    .map(|slot| {
                        10 + slot
                            .as_ref()
                            .and_then(|stack| stack.components.as_ref())
                            .map_or(0, |payload| payload.bytes.len())
                    })
                    .sum::<usize>()
            }
            ServerMessage::Drops { items, .. } => 8 + 2 + items.len() * DROP_ITEM,
            ServerMessage::Pickups { items } => 2 + items.len() * DROP_ITEM,
            ServerMessage::WorldSnapshotStart(start) => entities::snapshot_start_wire_len(start),
            ServerMessage::EntitySnapshotPage(page) => entities::snapshot_page_wire_len(page),
            ServerMessage::WorldCommitPart(part) => part.wire_len(),
            ServerMessage::OwnedEntity { .. } => 8,
        }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn frame(mut writer: impl Write, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME {
        return Err(invalid("frame too large"));
    }
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(payload)
}

fn read_frame(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut size = [0; 4];
    reader.read_exact(&mut size)?;
    let len = u32::from_le_bytes(size) as usize;
    if !(2..=MAX_FRAME).contains(&len) {
        return Err(invalid("invalid frame size"));
    }
    let mut payload = vec![0; len];
    reader.read_exact(&mut payload)?;
    if payload[0] != WIRE_VERSION {
        return Err(invalid("unsupported wire version"));
    }
    Ok(payload)
}

fn key(out: &mut Vec<u8>, value: ChunkKey) {
    out.extend(value.x.to_le_bytes());
    out.extend(value.y.to_le_bytes());
    out.extend(value.z.to_le_bytes());
}

fn short_string(out: &mut Vec<u8>, value: &str) -> io::Result<()> {
    if value.len() > MAX_NAME {
        return Err(invalid("string too long"));
    }
    out.push(value.len() as u8);
    out.extend(value.as_bytes());
    Ok(())
}

pub fn write_client(writer: impl Write, message: &ClientMessage) -> io::Result<()> {
    write_client_with_catalog(writer, message, crate::content::catalog())
}

pub fn write_client_with_catalog(
    writer: impl Write,
    message: &ClientMessage,
    content_catalog: &Catalog,
) -> io::Result<()> {
    let mut out = vec![WIRE_VERSION];
    match message {
        ClientMessage::Hello {
            name,
            profile,
            content_fingerprint,
        } => {
            out.push(1);
            short_string(&mut out, name)?;
            out.extend(profile.to_le_bytes());
            out.extend(content_fingerprint.to_le_bytes());
        }
        ClientMessage::Move { seq, dx, dy, dz } => {
            if !dx.is_finite() || !dy.is_finite() || !dz.is_finite() {
                return Err(invalid("nonfinite movement"));
            }
            out.push(2);
            out.extend(seq.to_le_bytes());
            for n in [dx, dy, dz] {
                out.extend(n.to_le_bytes());
            }
        }
        ClientMessage::Edit {
            action_id,
            x,
            y,
            z,
            block,
            slot,
        } => {
            if content_catalog.state(*block).is_none() {
                return Err(invalid("invalid block type"));
            }
            if !valid_action_id(*action_id) {
                return Err(invalid("invalid action ID"));
            }
            out.push(3);
            out.extend(action_id.to_le_bytes());
            for n in [x, y, z] {
                out.extend(n.to_le_bytes());
            }
            out.extend(block.0.to_le_bytes());
            out.push(*slot);
        }
        ClientMessage::Resync { key: k } => {
            out.push(4);
            key(&mut out, *k);
        }
        ClientMessage::SetView { radius } => {
            out.push(5);
            out.push(*radius);
        }
        ClientMessage::Ping { nonce } => {
            out.push(6);
            out.extend(nonce.to_le_bytes());
        }
        ClientMessage::InventoryMove {
            action_id,
            from,
            to,
            count,
        } => {
            if *from as usize >= SLOTS
                || *to as usize >= SLOTS
                || !(1..=STACK_LIMIT).contains(count)
                || !valid_action_id(*action_id)
            {
                return Err(invalid("invalid inventory move"));
            }
            out.push(7);
            out.extend(action_id.to_le_bytes());
            out.extend([*from, *to]);
            out.extend(count.to_le_bytes());
        }
        ClientMessage::DropStack {
            action_id,
            slot,
            count,
        } => {
            if *slot as usize >= SLOTS
                || !(1..=STACK_LIMIT).contains(count)
                || !valid_action_id(*action_id)
            {
                return Err(invalid("invalid dropped stack"));
            }
            out.push(8);
            out.extend(action_id.to_le_bytes());
            out.push(*slot);
            out.extend(count.to_le_bytes());
        }
        ClientMessage::AdminGive {
            action_id,
            item,
            count,
        } => {
            if !valid_action_id(*action_id)
                || content_catalog.item(*item).is_none()
                || !(1..=STACK_LIMIT).contains(count)
            {
                return Err(invalid("invalid admin grant"));
            }
            out.push(12);
            out.extend(action_id.to_le_bytes());
            out.extend(item.0.to_le_bytes());
            out.extend(count.to_le_bytes());
        }
        ClientMessage::ContentReady { fingerprint } => {
            out.push(9);
            out.extend(fingerprint.to_le_bytes());
        }
        ClientMessage::ActionAck { epoch, through_seq } => {
            if *epoch == 0 || *through_seq == 0 {
                return Err(invalid("invalid action acknowledgement"));
            }
            out.push(10);
            out.extend(epoch.to_le_bytes());
            out.extend(through_seq.to_le_bytes());
        }
        ClientMessage::EntityInteract {
            action_id,
            target,
            payload,
        } => {
            if !valid_action_id(*action_id)
                || payload.is_empty()
                || payload.len() > MAX_ENTITY_INTERACT_BYTES
            {
                return Err(invalid("invalid entity interaction"));
            }
            out.push(11);
            out.extend(action_id.to_le_bytes());
            for coordinate in target {
                out.extend(coordinate.to_le_bytes());
            }
            out.extend((payload.len() as u16).to_le_bytes());
            out.extend(payload);
        }
    }
    frame(writer, &out)
}

#[cfg(test)]
pub fn write_server(writer: impl Write, message: &ServerMessage) -> io::Result<()> {
    write_server_with_catalog(writer, message, crate::content::catalog())
}

pub fn write_server_with_catalog(
    writer: impl Write,
    message: &ServerMessage,
    content_catalog: &Catalog,
) -> io::Result<()> {
    let mut out = vec![WIRE_VERSION];
    match message {
        ServerMessage::Welcome { id, seed } => {
            out.push(1);
            out.extend(id.to_le_bytes());
            out.extend(seed.to_le_bytes());
        }
        ServerMessage::Position { ack_seq, x, y, z } => {
            out.push(2);
            out.extend(ack_seq.to_le_bytes());
            for n in [x, y, z] {
                out.extend(n.to_le_bytes());
            }
        }
        ServerMessage::Chunk(chunk) => {
            if chunk.blocks.len() != BLOCK_COUNT
                || chunk
                    .blocks
                    .iter()
                    .any(|&block| content_catalog.state(block).is_none())
            {
                return Err(invalid("invalid chunk size"));
            }
            out.push(3);
            key(&mut out, chunk.key);
            out.extend(chunk.version.to_le_bytes());
            write_paletted_blocks(&mut out, &chunk.blocks)?;
        }
        ServerMessage::Delta {
            key: k,
            version,
            x,
            y,
            z,
            block,
        } => {
            if [*x, *y, *z].iter().any(|n| *n as usize >= CHUNK_SIZE) {
                return Err(invalid("invalid local coordinate"));
            }
            if content_catalog.state(*block).is_none() {
                return Err(invalid("invalid block type"));
            }
            out.push(4);
            key(&mut out, *k);
            out.extend(version.to_le_bytes());
            out.extend([*x, *y, *z]);
            out.extend(block.0.to_le_bytes());
        }
        ServerMessage::EditRejected { reason } => {
            out.push(5);
            short_string(&mut out, reason)?;
        }
        ServerMessage::ActionResult {
            action_id,
            accepted,
            reason,
        } => {
            if !valid_action_id(*action_id) || (*accepted && !reason.is_empty()) {
                return Err(invalid("invalid action result"));
            }
            out.push(11);
            out.extend(action_id.to_le_bytes());
            out.push(u8::from(*accepted));
            short_string(&mut out, reason)?;
        }
        ServerMessage::ActionSession {
            epoch,
            next_seq,
            acked_seq,
        } => {
            if *epoch == 0 || *next_seq == 0 || *acked_seq >= *next_seq {
                return Err(invalid("invalid action session"));
            }
            out.push(13);
            out.extend(epoch.to_le_bytes());
            out.extend(next_seq.to_le_bytes());
            out.extend(acked_seq.to_le_bytes());
        }
        ServerMessage::ActionDeferred { action_id } => {
            if !valid_action_id(*action_id) {
                return Err(invalid("invalid deferred action"));
            }
            out.push(14);
            out.extend(action_id.to_le_bytes());
        }
        ServerMessage::Pong { nonce } => {
            out.push(6);
            out.extend(nonce.to_le_bytes());
        }
        ServerMessage::ViewDistance { radius } => {
            if !(MIN_VIEW_DISTANCE..=MAX_VIEW_DISTANCE).contains(radius) {
                return Err(invalid("invalid view distance"));
            }
            out.push(7);
            out.push(*radius);
        }
        ServerMessage::Inventory { revision, slots } => {
            out.push(8);
            out.extend(revision.to_le_bytes());
            for slot in slots {
                if let Some(stack) = slot {
                    if !stack.valid_in(content_catalog) {
                        return Err(invalid("invalid inventory stack"));
                    }
                    out.extend(stack.item.0.to_le_bytes());
                    out.extend(stack.count.to_le_bytes());
                    if let Some(payload) = &stack.components {
                        out.extend(payload.version.to_le_bytes());
                        out.extend((payload.bytes.len() as u16).to_le_bytes());
                        out.extend(&payload.bytes);
                    } else {
                        out.extend([0; 4]);
                    }
                } else {
                    out.extend([0; 10]);
                }
            }
        }
        ServerMessage::Drops { revision, items } => {
            out.push(9);
            out.extend(revision.to_le_bytes());
            write_drop_items(&mut out, items, content_catalog)?;
        }
        ServerMessage::Pickups { items } => {
            out.push(10);
            write_drop_items(&mut out, items, content_catalog)?;
        }
        ServerMessage::ContentManifestPart {
            fingerprint,
            total_len,
            offset,
            bytes,
        } => {
            if bytes.is_empty()
                || bytes.len() > MAX_MANIFEST_PART
                || *total_len as usize > MAX_MANIFEST_BYTES
                || offset
                    .checked_add(bytes.len() as u32)
                    .is_none_or(|end| end > *total_len)
            {
                return Err(invalid("invalid manifest part"));
            }
            out.push(12);
            out.extend(fingerprint.to_le_bytes());
            out.extend(total_len.to_le_bytes());
            out.extend(offset.to_le_bytes());
            out.extend((bytes.len() as u16).to_le_bytes());
            out.extend(bytes);
        }
        ServerMessage::WorldSnapshotStart(start) => {
            out.push(15);
            entities::write_snapshot_start(&mut out, start, content_catalog)?;
        }
        ServerMessage::EntitySnapshotPage(page) => {
            out.push(16);
            entities::write_snapshot_page(&mut out, page, content_catalog)?;
        }
        ServerMessage::WorldCommitPart(part) => {
            out.push(17);
            entities::write_commit_part(&mut out, part, content_catalog)?;
        }
        ServerMessage::OwnedEntity { id } => {
            if *id == 0 {
                return Err(invalid("invalid owned entity id"));
            }
            out.push(18);
            out.extend(id.to_le_bytes());
        }
    }
    entities::enforce_frame_size(&out)?;
    frame(writer, &out)
}

fn write_drop_items(
    out: &mut Vec<u8>,
    items: &[DroppedItem],
    content_catalog: &Catalog,
) -> io::Result<()> {
    if items.len() > 256 {
        return Err(invalid("too many drops"));
    }
    out.extend((items.len() as u16).to_le_bytes());
    for item in items {
        if content_catalog.item(item.item).is_none()
            || !(1..=STACK_LIMIT).contains(&item.count)
            || item.position.iter().any(|n| !n.is_finite())
        {
            return Err(invalid("invalid dropped item"));
        }
        out.extend(item.id.to_le_bytes());
        out.extend(item.item.0.to_le_bytes());
        out.extend(item.count.to_le_bytes());
        for n in item.position {
            out.extend(n.to_le_bytes());
        }
        out.extend(item.age_ms.to_le_bytes());
    }
    Ok(())
}

/// Wire v8 chunk payload: count u16, width u8, first-seen state IDs u32,
/// then 4,096 local indices. Width is one byte through 256 entries and two
/// bytes thereafter. A maximal 4,096-state chunk remains below 64 KiB.
#[cfg(test)]
fn write_palette(out: &mut Vec<u8>, blocks: &[BlockStateId]) -> io::Result<()> {
    if blocks.len() != BLOCK_COUNT {
        return Err(invalid("invalid chunk size"));
    }
    write_paletted_blocks(out, &PalettedBlocks::from(blocks.to_vec()))?;
    Ok(())
}

fn write_paletted_blocks(out: &mut Vec<u8>, blocks: &PalettedBlocks) -> io::Result<()> {
    if blocks.len() != BLOCK_COUNT {
        return Err(invalid("invalid chunk size"));
    }
    match blocks.view() {
        PaletteView::Uniform(state) => {
            out.extend(1u16.to_le_bytes());
            out.push(1);
            out.extend(state.0.to_le_bytes());
            out.resize(out.len() + BLOCK_COUNT, 0);
        }
        PaletteView::Palette8 { palette, indices } => {
            out.extend((palette.len() as u16).to_le_bytes());
            out.push(1);
            for state in palette {
                out.extend(state.0.to_le_bytes());
            }
            out.extend(indices);
        }
        PaletteView::Palette16 { palette, indices } => {
            out.extend((palette.len() as u16).to_le_bytes());
            out.push(2);
            for state in palette {
                out.extend(state.0.to_le_bytes());
            }
            for index in indices {
                out.extend(index.to_le_bytes());
            }
        }
        PaletteView::InvalidLength(_) => return Err(invalid("invalid chunk size")),
    }
    Ok(())
}

fn read_palette_with(
    c: &mut Cursor<'_>,
    valid: impl Fn(BlockStateId) -> bool,
) -> io::Result<Vec<BlockStateId>> {
    let count = c.u16()? as usize;
    let width = c.u8()?;
    if !(1..=BLOCK_COUNT).contains(&count) || width != if count <= 256 { 1 } else { 2 } {
        return Err(invalid("invalid chunk palette"));
    }
    let mut palette = Vec::with_capacity(count);
    let mut seen = std::collections::HashSet::with_capacity(count);
    for _ in 0..count {
        let state = BlockStateId(c.u32()?);
        if !valid(state) || !seen.insert(state) {
            return Err(invalid("invalid chunk palette state"));
        }
        palette.push(state);
    }
    let mut blocks = Vec::with_capacity(BLOCK_COUNT);
    let mut used = vec![false; count];
    for _ in 0..BLOCK_COUNT {
        let index = if width == 1 {
            c.u8()? as usize
        } else {
            c.u16()? as usize
        };
        let Some(&state) = palette.get(index) else {
            return Err(invalid("invalid chunk palette index"));
        };
        used[index] = true;
        blocks.push(state);
    }
    if used.iter().any(|used| !used) {
        return Err(invalid("unused chunk palette entry"));
    }
    Ok(blocks)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 2 }
    }
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| invalid("truncated frame"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| invalid("truncated frame"))?;
        self.offset = end;
        Ok(value)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u128(&mut self) -> io::Result<u128> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> io::Result<f32> {
        let value = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if !value.is_finite() {
            return Err(invalid("nonfinite float"));
        }
        Ok(value)
    }
    fn key(&mut self) -> io::Result<ChunkKey> {
        Ok(ChunkKey {
            x: self.i32()?,
            y: self.i32()?,
            z: self.i32()?,
        })
    }
    fn string(&mut self) -> io::Result<String> {
        let len = self.u8()? as usize;
        if len > MAX_NAME {
            return Err(invalid("string too long"));
        }
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| invalid("invalid UTF-8"))
    }
    fn done(&self) -> io::Result<()> {
        if self.offset != self.bytes.len() {
            Err(invalid("trailing bytes"))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
pub fn read_client(reader: impl Read) -> io::Result<ClientMessage> {
    read_client_with_catalog(reader, crate::content::catalog())
}

pub fn read_client_with_catalog(
    reader: impl Read,
    content_catalog: &Catalog,
) -> io::Result<ClientMessage> {
    let bytes = read_frame(reader)?;
    let mut c = Cursor::new(&bytes);
    let message = match bytes[1] {
        1 => ClientMessage::Hello {
            name: c.string()?,
            profile: c.u128()?,
            content_fingerprint: c.u64()?,
        },
        2 => ClientMessage::Move {
            seq: c.u64()?,
            dx: c.f32()?,
            dy: c.f32()?,
            dz: c.f32()?,
        },
        3 => {
            let (action_id, x, y, z, block, slot) = (
                c.u128()?,
                c.i32()?,
                c.i32()?,
                c.i32()?,
                BlockStateId(c.u32()?),
                c.u8()?,
            );
            if content_catalog.state(block).is_none() || !valid_action_id(action_id) {
                return Err(invalid("invalid block type"));
            }
            ClientMessage::Edit {
                action_id,
                x,
                y,
                z,
                block,
                slot,
            }
        }
        4 => ClientMessage::Resync { key: c.key()? },
        5 => ClientMessage::SetView { radius: c.u8()? },
        6 => ClientMessage::Ping { nonce: c.u64()? },
        7 => {
            let (action_id, from, to, count) = (c.u128()?, c.u8()?, c.u8()?, c.u16()?);
            if from as usize >= SLOTS
                || to as usize >= SLOTS
                || !(1..=STACK_LIMIT).contains(&count)
                || !valid_action_id(action_id)
            {
                return Err(invalid("invalid inventory move"));
            }
            ClientMessage::InventoryMove {
                action_id,
                from,
                to,
                count,
            }
        }
        8 => {
            let (action_id, slot, count) = (c.u128()?, c.u8()?, c.u16()?);
            if slot as usize >= SLOTS
                || !(1..=STACK_LIMIT).contains(&count)
                || !valid_action_id(action_id)
            {
                return Err(invalid("invalid dropped stack"));
            }
            ClientMessage::DropStack {
                action_id,
                slot,
                count,
            }
        }
        9 => ClientMessage::ContentReady {
            fingerprint: c.u64()?,
        },
        10 => {
            let epoch = c.u64()?;
            let through_seq = c.u64()?;
            if epoch == 0 || through_seq == 0 {
                return Err(invalid("invalid action acknowledgement"));
            }
            ClientMessage::ActionAck { epoch, through_seq }
        }
        11 => {
            let action_id = c.u128()?;
            let target = [c.i32()?, c.i32()?, c.i32()?];
            let len = usize::from(c.u16()?);
            if !valid_action_id(action_id) || !(1..=MAX_ENTITY_INTERACT_BYTES).contains(&len) {
                return Err(invalid("invalid entity interaction"));
            }
            ClientMessage::EntityInteract {
                action_id,
                target,
                payload: c.take(len)?.to_vec(),
            }
        }
        12 => {
            let action_id = c.u128()?;
            let item = ItemId(c.u32()?);
            let count = c.u16()?;
            if !valid_action_id(action_id)
                || content_catalog.item(item).is_none()
                || !(1..=STACK_LIMIT).contains(&count)
            {
                return Err(invalid("invalid admin grant"));
            }
            ClientMessage::AdminGive {
                action_id,
                item,
                count,
            }
        }
        _ => return Err(invalid("unknown client message")),
    };
    c.done()?;
    Ok(message)
}

pub fn read_server(reader: impl Read) -> io::Result<ServerMessage> {
    read_server_with_catalog(reader, crate::content::catalog())
}

pub fn read_server_with_catalog(
    reader: impl Read,
    content_catalog: &Catalog,
) -> io::Result<ServerMessage> {
    let bytes = read_frame(reader)?;
    let mut c = Cursor::new(&bytes);
    let message = match bytes[1] {
        1 => ServerMessage::Welcome {
            id: c.u64()?,
            seed: c.u64()?,
        },
        2 => ServerMessage::Position {
            ack_seq: c.u64()?,
            x: c.f32()?,
            y: c.f32()?,
            z: c.f32()?,
        },
        3 => {
            let key = c.key()?;
            let version = c.u64()?;
            let blocks = read_palette_with(&mut c, |state| content_catalog.state(state).is_some())?;
            ServerMessage::Chunk(Chunk::from_blocks(key, version, blocks))
        }
        4 => {
            let key = c.key()?;
            let version = c.u64()?;
            let (x, y, z, block) = (c.u8()?, c.u8()?, c.u8()?, BlockStateId(c.u32()?));
            if [x, y, z].iter().any(|n| *n as usize >= CHUNK_SIZE)
                || content_catalog.state(block).is_none()
            {
                return Err(invalid("invalid local coordinate"));
            }
            ServerMessage::Delta {
                key,
                version,
                x,
                y,
                z,
                block,
            }
        }
        5 => ServerMessage::EditRejected {
            reason: c.string()?,
        },
        6 => ServerMessage::Pong { nonce: c.u64()? },
        7 => {
            let radius = c.u8()?;
            if !(MIN_VIEW_DISTANCE..=MAX_VIEW_DISTANCE).contains(&radius) {
                return Err(invalid("invalid view distance"));
            }
            ServerMessage::ViewDistance { radius }
        }
        8 => {
            let revision = c.u64()?;
            let mut slots = std::array::from_fn(|_| None);
            for slot in &mut slots {
                let item = ItemId(c.u32()?);
                let count = c.u16()?;
                let component_version = c.u16()?;
                let component_len = c.u16()? as usize;
                if component_len > MAX_COMPONENT_BYTES {
                    return Err(invalid("invalid inventory components"));
                }
                if item.0 == 0 {
                    if count != 0 || component_version != 0 || component_len != 0 {
                        return Err(invalid("invalid empty inventory slot"));
                    }
                } else {
                    let stack = if component_len == 0 {
                        if component_version != 0 {
                            return Err(invalid("invalid inventory component version"));
                        }
                        Stack::new(item, count)
                    } else {
                        Stack::with_components(
                            item,
                            count,
                            component_version,
                            c.take(component_len)?.to_vec(),
                        )
                        .ok_or_else(|| invalid("invalid inventory components"))?
                    };
                    if !stack.valid_in(content_catalog) {
                        return Err(invalid("invalid inventory stack"));
                    }
                    *slot = Some(stack);
                }
            }
            ServerMessage::Inventory { revision, slots }
        }
        9 => {
            let revision = c.u64()?;
            let count = c.u16()? as usize;
            if count > 256 {
                return Err(invalid("too many drops"));
            }
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                let item = DroppedItem {
                    id: c.u64()?,
                    item: ItemId(c.u32()?),
                    count: c.u16()?,
                    position: [c.f32()?, c.f32()?, c.f32()?],
                    age_ms: c.u32()?,
                };
                if content_catalog.item(item.item).is_none()
                    || !(1..=STACK_LIMIT).contains(&item.count)
                {
                    return Err(invalid("invalid dropped item"));
                }
                items.push(item);
            }
            ServerMessage::Drops { revision, items }
        }
        10 => {
            let count = c.u16()? as usize;
            if count > 256 {
                return Err(invalid("too many pickups"));
            }
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                let item = DroppedItem {
                    id: c.u64()?,
                    item: ItemId(c.u32()?),
                    count: c.u16()?,
                    position: [c.f32()?, c.f32()?, c.f32()?],
                    age_ms: c.u32()?,
                };
                if content_catalog.item(item.item).is_none()
                    || !(1..=STACK_LIMIT).contains(&item.count)
                {
                    return Err(invalid("invalid picked-up item"));
                }
                items.push(item);
            }
            ServerMessage::Pickups { items }
        }
        11 => {
            let action_id = c.u128()?;
            let accepted = match c.u8()? {
                0 => false,
                1 => true,
                _ => return Err(invalid("invalid action result")),
            };
            let reason = c.string()?;
            if !valid_action_id(action_id) || (accepted && !reason.is_empty()) {
                return Err(invalid("invalid action result"));
            }
            ServerMessage::ActionResult {
                action_id,
                accepted,
                reason,
            }
        }
        12 => {
            let fingerprint = c.u64()?;
            let total_len = c.u32()?;
            let offset = c.u32()?;
            let part_len = c.u16()? as usize;
            if part_len == 0
                || part_len > MAX_MANIFEST_PART
                || total_len as usize > MAX_MANIFEST_BYTES
                || offset
                    .checked_add(part_len as u32)
                    .is_none_or(|end| end > total_len)
            {
                return Err(invalid("invalid manifest part"));
            }
            let bytes = c.take(part_len)?.to_vec();
            ServerMessage::ContentManifestPart {
                fingerprint,
                total_len,
                offset,
                bytes,
            }
        }
        13 => {
            let epoch = c.u64()?;
            let next_seq = c.u64()?;
            let acked_seq = c.u64()?;
            if epoch == 0 || next_seq == 0 || acked_seq >= next_seq {
                return Err(invalid("invalid action session"));
            }
            ServerMessage::ActionSession {
                epoch,
                next_seq,
                acked_seq,
            }
        }
        14 => {
            let action_id = c.u128()?;
            if !valid_action_id(action_id) {
                return Err(invalid("invalid deferred action"));
            }
            ServerMessage::ActionDeferred { action_id }
        }
        15 => ServerMessage::WorldSnapshotStart(entities::read_snapshot_start(
            &mut c,
            content_catalog,
        )?),
        16 => ServerMessage::EntitySnapshotPage(entities::read_snapshot_page(
            &mut c,
            content_catalog,
        )?),
        17 => ServerMessage::WorldCommitPart(entities::read_commit_part(&mut c, content_catalog)?),
        18 => {
            let id = c.u64()?;
            if id == 0 {
                return Err(invalid("invalid owned entity id"));
            }
            ServerMessage::OwnedEntity { id }
        }
        _ => return Err(invalid("unknown server message")),
    };
    c.done()?;
    Ok(message)
}

#[cfg(test)]
#[path = "protocol/tests.rs"]
mod tests;

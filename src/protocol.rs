//! Small, versioned, length-prefixed wire format shared by the client and server.
use crate::inventory::{SLOTS, STACK_LIMIT, Stack};
use crate::items::{ItemId, valid_item};
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey, valid_block};
use std::io::{self, Read, Write};

pub const MAX_FRAME: usize = 16 * 1024;
const WIRE_VERSION: u8 = 7;
pub const MIN_VIEW_DISTANCE: u8 = 1;
pub const MAX_VIEW_DISTANCE: u8 = 6;
const MAX_NAME: usize = 32;
const BLOCK_COUNT: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

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
        block: u8,
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
    Resync {
        key: ChunkKey,
    },
    SetView {
        radius: u8,
    },
    Ping {
        nonce: u64,
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
        block: u8,
    },
    EditRejected {
        reason: String,
    },
    ActionResult {
        action_id: u128,
        accepted: bool,
        reason: String,
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
            if !valid_block(*block) {
                return Err(invalid("invalid block type"));
            }
            if *action_id == 0 {
                return Err(invalid("invalid action ID"));
            }
            out.push(3);
            out.extend(action_id.to_le_bytes());
            for n in [x, y, z] {
                out.extend(n.to_le_bytes());
            }
            out.push(*block);
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
                || *action_id == 0
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
            if *slot as usize >= SLOTS || !(1..=STACK_LIMIT).contains(count) || *action_id == 0 {
                return Err(invalid("invalid dropped stack"));
            }
            out.push(8);
            out.extend(action_id.to_le_bytes());
            out.push(*slot);
            out.extend(count.to_le_bytes());
        }
    }
    frame(writer, &out)
}

pub fn write_server(writer: impl Write, message: &ServerMessage) -> io::Result<()> {
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
                || chunk.blocks.iter().any(|&block| !valid_block(block))
            {
                return Err(invalid("invalid chunk size"));
            }
            out.push(3);
            key(&mut out, chunk.key);
            out.extend(chunk.version.to_le_bytes());
            out.extend(&chunk.blocks);
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
            if !valid_block(*block) {
                return Err(invalid("invalid block type"));
            }
            out.push(4);
            key(&mut out, *k);
            out.extend(version.to_le_bytes());
            out.extend([*x, *y, *z, *block]);
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
            if *action_id == 0 || (accepted && !reason.is_empty()) {
                return Err(invalid("invalid action result"));
            }
            out.push(11);
            out.extend(action_id.to_le_bytes());
            out.push(u8::from(*accepted));
            short_string(&mut out, reason)?;
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
                    if !stack.valid() {
                        return Err(invalid("invalid inventory stack"));
                    }
                    out.push(stack.item);
                    out.extend(stack.count.to_le_bytes());
                } else {
                    out.extend([0, 0, 0]);
                }
            }
        }
        ServerMessage::Drops { revision, items } => {
            out.push(9);
            out.extend(revision.to_le_bytes());
            write_drop_items(&mut out, items)?;
        }
        ServerMessage::Pickups { items } => {
            out.push(10);
            write_drop_items(&mut out, items)?;
        }
    }
    frame(writer, &out)
}

fn write_drop_items(out: &mut Vec<u8>, items: &[DroppedItem]) -> io::Result<()> {
    if items.len() > 256 {
        return Err(invalid("too many drops"));
    }
    out.extend((items.len() as u16).to_le_bytes());
    for item in items {
        if !valid_item(item.item)
            || !(1..=STACK_LIMIT).contains(&item.count)
            || item.position.iter().any(|n| !n.is_finite())
        {
            return Err(invalid("invalid dropped item"));
        }
        out.extend(item.id.to_le_bytes());
        out.push(item.item);
        out.extend(item.count.to_le_bytes());
        for n in item.position {
            out.extend(n.to_le_bytes());
        }
        out.extend(item.age_ms.to_le_bytes());
    }
    Ok(())
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

pub fn read_client(reader: impl Read) -> io::Result<ClientMessage> {
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
            let (action_id, x, y, z, block, slot) =
                (c.u128()?, c.i32()?, c.i32()?, c.i32()?, c.u8()?, c.u8()?);
            if !valid_block(block) || action_id == 0 {
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
                || action_id == 0
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
            if slot as usize >= SLOTS || !(1..=STACK_LIMIT).contains(&count) || action_id == 0 {
                return Err(invalid("invalid dropped stack"));
            }
            ClientMessage::DropStack {
                action_id,
                slot,
                count,
            }
        }
        _ => return Err(invalid("unknown client message")),
    };
    c.done()?;
    Ok(message)
}

pub fn read_server(reader: impl Read) -> io::Result<ServerMessage> {
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
            let blocks = c.take(BLOCK_COUNT)?.to_vec();
            if blocks.iter().any(|&block| !valid_block(block)) {
                return Err(invalid("unknown block in chunk"));
            }
            ServerMessage::Chunk(Chunk {
                key,
                version,
                blocks,
            })
        }
        4 => {
            let key = c.key()?;
            let version = c.u64()?;
            let (x, y, z, block) = (c.u8()?, c.u8()?, c.u8()?, c.u8()?);
            if [x, y, z].iter().any(|n| *n as usize >= CHUNK_SIZE) || !valid_block(block) {
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
            let mut slots = [None; SLOTS];
            for slot in &mut slots {
                let item = c.u8()?;
                let count = c.u16()?;
                if item != 0 || count != 0 {
                    let stack = Stack { item, count };
                    if !stack.valid() {
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
                    item: c.u8()?,
                    count: c.u16()?,
                    position: [c.f32()?, c.f32()?, c.f32()?],
                    age_ms: c.u32()?,
                };
                if !valid_item(item.item) || !(1..=STACK_LIMIT).contains(&item.count) {
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
                    item: c.u8()?,
                    count: c.u16()?,
                    position: [c.f32()?, c.f32()?, c.f32()?],
                    age_ms: c.u32()?,
                };
                if !valid_item(item.item) || !(1..=STACK_LIMIT).contains(&item.count) {
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
            if action_id == 0 || (accepted && !reason.is_empty()) {
                return Err(invalid("invalid action result"));
            }
            ServerMessage::ActionResult {
                action_id,
                accepted,
                reason,
            }
        }
        _ => return Err(invalid("unknown server message")),
    };
    c.done()?;
    Ok(message)
}

#[cfg(test)]
#[path = "protocol/tests.rs"]
mod tests;

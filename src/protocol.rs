//! Small, versioned, length-prefixed wire format shared by the client and server.
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey};
use std::io::{self, Read, Write};

pub const MAX_FRAME: usize = 16 * 1024;
const WIRE_VERSION: u8 = 2;
pub const MIN_VIEW_DISTANCE: u8 = 1;
pub const MAX_VIEW_DISTANCE: u8 = 6;
const MAX_NAME: usize = 32;
const BLOCK_COUNT: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

#[derive(Debug, Clone, PartialEq)]
pub enum ClientMessage {
    Hello { name: String },
    Move { seq: u64, dx: f32, dy: f32, dz: f32 },
    Edit { x: i32, y: i32, z: i32, block: u8 },
    Resync { key: ChunkKey },
    SetView { radius: u8 },
    Ping { nonce: u64 },
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
    ViewDistance {
        radius: u8,
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
        ClientMessage::Hello { name } => {
            out.push(1);
            short_string(&mut out, name)?;
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
        ClientMessage::Edit { x, y, z, block } => {
            out.push(3);
            for n in [x, y, z] {
                out.extend(n.to_le_bytes());
            }
            out.push(*block);
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
            if chunk.blocks.len() != BLOCK_COUNT {
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
            out.push(4);
            key(&mut out, *k);
            out.extend(version.to_le_bytes());
            out.extend([*x, *y, *z, *block]);
        }
        ServerMessage::EditRejected { reason } => {
            out.push(5);
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
    }
    frame(writer, &out)
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
        1 => ClientMessage::Hello { name: c.string()? },
        2 => ClientMessage::Move {
            seq: c.u64()?,
            dx: c.f32()?,
            dy: c.f32()?,
            dz: c.f32()?,
        },
        3 => ClientMessage::Edit {
            x: c.i32()?,
            y: c.i32()?,
            z: c.i32()?,
            block: c.u8()?,
        },
        4 => ClientMessage::Resync { key: c.key()? },
        5 => ClientMessage::SetView { radius: c.u8()? },
        6 => ClientMessage::Ping { nonce: c.u64()? },
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
            if [x, y, z].iter().any(|n| *n as usize >= CHUNK_SIZE) {
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
        _ => return Err(invalid("unknown server message")),
    };
    c.done()?;
    Ok(message)
}

#[cfg(test)]
#[path = "protocol/tests.rs"]
mod tests;

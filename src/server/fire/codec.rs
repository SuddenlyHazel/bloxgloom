//! Small strict codecs shared by fire's three persistent domains.

use crate::world::ChunkKey;
use std::io::{self, ErrorKind};

pub(super) fn key_bytes(key: ChunkKey) -> [u8; 12] {
    let mut bytes = [0; 12];
    bytes[..4].copy_from_slice(&key.x.to_le_bytes());
    bytes[4..8].copy_from_slice(&key.y.to_le_bytes());
    bytes[8..12].copy_from_slice(&key.z.to_le_bytes());
    bytes
}

pub(super) fn read_key(bytes: &[u8]) -> io::Result<ChunkKey> {
    if bytes.len() != 12 {
        return Err(invalid("invalid fire chunk key"));
    }
    Ok(ChunkKey {
        x: i32::from_le_bytes(bytes[..4].try_into().unwrap()),
        y: i32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        z: i32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    })
}

pub(super) fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}

pub(super) fn finish(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

pub(super) fn checked_body<'a>(
    bytes: &'a [u8],
    magic: &[u8; 4],
    record: usize,
) -> io::Result<&'a [u8]> {
    if bytes.len() < 11 || &bytes[..4] != magic || bytes[4] != 1 {
        return Err(invalid("invalid fire record header"));
    }
    let count = usize::from(u16::from_le_bytes(bytes[5..7].try_into().unwrap()));
    let expected = 7usize
        .checked_add(
            count
                .checked_mul(record)
                .ok_or_else(|| invalid("fire record too large"))?,
        )
        .and_then(|size| size.checked_add(4))
        .ok_or_else(|| invalid("fire record too large"))?;
    if bytes.len() != expected {
        return Err(invalid("invalid fire record length"));
    }
    let body_end = bytes.len() - 4;
    if checksum(&bytes[..body_end]) != u32::from_le_bytes(bytes[body_end..].try_into().unwrap()) {
        return Err(invalid("fire record checksum mismatch"));
    }
    Ok(&bytes[7..body_end])
}

pub(super) fn invalid(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

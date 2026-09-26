//! Worker-only streaming checkpoint frames. No whole-generation output buffer.
//!
//! A turn visits at most `quota` entries and stops after reaching 64 KiB (one
//! bounded entry can overshoot); each producer must independently
//! bound one entry before allocating it. CRC and writes are split into 64 KiB
//! pieces. Filesystem calls (including final fsync) have no latency guarantee.

use std::io::{self, Write};

pub(super) const TURN_ENTRIES: usize = 16;
const WRITE_BYTES: usize = 64 * 1024;

pub(super) fn write_frame(
    output: &mut impl Write,
    limit: usize,
    mut parts: impl Iterator<Item = io::Result<Vec<u8>>>,
    quota: usize,
    mut after_turn: impl FnMut(usize) -> io::Result<()>,
) -> io::Result<()> {
    if quota == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "zero checkpoint quota",
        ));
    }
    let mut crc = !0u32;
    let mut total = 4usize; // final checksum
    loop {
        let mut count = 0;
        let mut turn_bytes = 0;
        for part in parts.by_ref().take(quota) {
            let part = part?;
            total = total
                .checked_add(part.len())
                .filter(|size| *size <= limit)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "checkpoint exceeds size limit")
                })?;
            for bytes in part.chunks(WRITE_BYTES) {
                for byte in bytes {
                    crc ^= u32::from(*byte);
                    for _ in 0..8 {
                        crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
                    }
                }
                output.write_all(bytes)?;
            }
            count += 1;
            turn_bytes += part.len();
            if turn_bytes >= WRITE_BYTES {
                break;
            }
        }
        if count == 0 {
            break;
        }
        after_turn(count)?;
    }
    output.write_all(&(!crc).to_le_bytes())
}

#[cfg(test)]
mod tests;

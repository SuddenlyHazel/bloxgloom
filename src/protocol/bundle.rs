//! One offered canonical artifact per connection. TCP ordering binds parts to
//! that offer; contiguous offsets and the final SHA-256 bind the exact bytes.
use super::{Cursor, ServerMessage, invalid};
use crate::server::client_bundle::{CacheKey, MAX_BUNDLE_BYTES};
use std::io;

pub const MAX_BUNDLE_PART: usize = 60 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BundleIdentity {
    pub key: CacheKey,
    pub total_len: u32,
}

impl BundleIdentity {
    pub fn validate(self) -> io::Result<()> {
        if self.total_len == 0 || self.total_len as usize > MAX_BUNDLE_BYTES {
            return Err(invalid("invalid client bundle length"));
        }
        Ok(())
    }
}

pub(super) fn write_identity(out: &mut Vec<u8>, identity: &BundleIdentity) -> io::Result<()> {
    identity.validate()?;
    out.extend(identity.key.as_bytes());
    out.extend(identity.total_len.to_le_bytes());
    Ok(())
}

pub(super) fn read_identity(c: &mut Cursor<'_>) -> io::Result<BundleIdentity> {
    let identity = BundleIdentity {
        key: CacheKey::from_bytes(c.take(32)?.try_into().expect("32 bytes")),
        total_len: c.u32()?,
    };
    identity.validate()?;
    Ok(identity)
}

fn validate_part(offset: u32, len: usize) -> io::Result<()> {
    if len == 0
        || len > MAX_BUNDLE_PART
        || (offset as usize)
            .checked_add(len)
            .is_none_or(|end| end > MAX_BUNDLE_BYTES)
    {
        return Err(invalid("invalid client bundle part"));
    }
    Ok(())
}

pub(super) fn write_part(out: &mut Vec<u8>, offset: u32, bytes: &[u8]) -> io::Result<()> {
    validate_part(offset, bytes.len())?;
    out.extend(offset.to_le_bytes());
    out.extend((bytes.len() as u16).to_le_bytes());
    out.extend(bytes);
    Ok(())
}

pub(super) fn read_part(c: &mut Cursor<'_>) -> io::Result<ServerMessage> {
    let offset = c.u32()?;
    let len = c.u16()? as usize;
    validate_part(offset, len)?;
    Ok(ServerMessage::BundlePart {
        offset,
        bytes: c.take(len)?.to_vec(),
    })
}

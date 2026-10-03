//! Bounded validation and evidence-based compatibility errors for world.meta.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use super::{
    MAX_GENERATION_IDENTITY_BYTES, SAVE_FORMAT_VERSION, TERRAIN_GENERATOR_VERSION, WORLD_MAGIC,
};

const GUIDANCE: &str = "restore the compatible package revision or use a new world directory (existing save left unchanged; no automatic upgrade)";

pub(super) fn read_world_metadata(path: &Path, identity: &[u8]) -> io::Result<u64> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((16 + MAX_GENERATION_IDENTITY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() == 14 && &bytes[..4] == WORLD_MAGIC {
        return Err(invalid(format!("unsupported old save format; {GUIDANCE}")));
    }
    if bytes.len() < 16
        || bytes.len() > 16 + MAX_GENERATION_IDENTITY_BYTES
        || &bytes[..4] != WORLD_MAGIC
    {
        return Err(invalid("invalid world metadata"));
    }
    let format = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
    if format != SAVE_FORMAT_VERSION {
        return Err(invalid(format!(
            "unsupported save format: saved {format}, current {SAVE_FORMAT_VERSION}; {GUIDANCE}",
        )));
    }
    let terrain = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    if terrain != TERRAIN_GENERATOR_VERSION {
        return Err(invalid(format!(
            "incompatible terrain generator 'bloxgloom:terrain': saved revision {terrain}, current revision {TERRAIN_GENERATOR_VERSION}; {GUIDANCE}",
        )));
    }
    let saved = decode_identity(&bytes[16..])?;
    let current = decode_identity(identity)?;
    if let Some((key, previous)) = saved
        .iter()
        .find(|(key, previous)| current.get(*key) != Some(*previous))
    {
        let namespace = key.split_once(':').map_or(key.as_str(), |(name, _)| name);
        let reason = match current.get(key) {
            None => format!(
                "saved contributor is missing from the current installation (saved revision {}, source {})",
                previous.revision,
                source(previous.source)
            ),
            Some(now) if now.revision != previous.revision => format!(
                "declared revision changed: saved {}, current {} (saved source {}, current source {})",
                previous.revision,
                now.revision,
                source(previous.source),
                source(now.source),
            ),
            Some(now) => format!(
                "source/dependency fingerprint changed at declared revision {}: saved {}, current {}",
                previous.revision,
                source(previous.source),
                source(now.source),
            ),
        };
        return Err(invalid(format!(
            "incompatible generation contributor '{key}' (namespace '{namespace}'): {reason}; {GUIDANCE}"
        )));
    }
    if let Some((key, now)) = current.iter().find(|(key, _)| !saved.contains_key(*key)) {
        let namespace = key.split_once(':').map_or(key.as_str(), |(name, _)| name);
        return Err(invalid(format!(
            "incompatible generation contributor '{key}' (namespace '{namespace}'): contributor added to the current installation, absent from save (current revision {}, source {}); {GUIDANCE}",
            now.revision,
            source(now.source),
        )));
    }
    Ok(u64::from_le_bytes(bytes[8..16].try_into().unwrap()))
}

#[derive(Debug, PartialEq, Eq)]
struct ContributorIdentity {
    revision: u32,
    source: Option<[u8; 32]>,
}

fn decode_identity(bytes: &[u8]) -> io::Result<BTreeMap<String, ContributorIdentity>> {
    let invalid_identity = || invalid("invalid generation identity in world metadata");
    if bytes.len() < 2 || bytes.len() > MAX_GENERATION_IDENTITY_BYTES {
        return Err(invalid_identity());
    }
    let count = usize::from(u16::from_le_bytes(bytes[..2].try_into().unwrap()));
    if count > 256 {
        return Err(invalid_identity());
    }
    let mut offset = 2;
    let mut entries = BTreeMap::new();
    let mut previous = None::<String>;
    for _ in 0..count {
        let len = usize::from(*bytes.get(offset).ok_or_else(invalid_identity)?);
        offset += 1;
        let key_bytes = bytes
            .get(offset..offset + len)
            .ok_or_else(invalid_identity)?;
        let key = std::str::from_utf8(key_bytes).map_err(|_| invalid_identity())?;
        if !valid_key(key) || previous.as_deref().is_some_and(|old| old >= key) {
            return Err(invalid_identity());
        }
        offset += len;
        let revision = u32::from_le_bytes(
            bytes
                .get(offset..offset + 4)
                .ok_or_else(invalid_identity)?
                .try_into()
                .unwrap(),
        );
        if revision == 0 {
            return Err(invalid_identity());
        }
        offset += 4;
        let source = match bytes.get(offset).ok_or_else(invalid_identity)? {
            0 => {
                offset += 1;
                None
            }
            1 => {
                offset += 1;
                let value = bytes
                    .get(offset..offset + 32)
                    .ok_or_else(invalid_identity)?
                    .try_into()
                    .unwrap();
                offset += 32;
                Some(value)
            }
            _ => return Err(invalid_identity()),
        };
        previous = Some(key.to_owned());
        entries.insert(key.to_owned(), ContributorIdentity { revision, source });
    }
    if offset != bytes.len() {
        return Err(invalid_identity());
    }
    Ok(entries)
}

fn valid_key(key: &str) -> bool {
    let Some((namespace, name)) = key.split_once(':') else {
        return false;
    };
    !namespace.is_empty()
        && !name.is_empty()
        && namespace
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'_' | b'-' | b'.' | b'/')
        })
}

fn source(source: Option<[u8; 32]>) -> String {
    source.map_or_else(
        || "native (no source digest)".into(),
        |digest| {
            use std::fmt::Write;
            let mut result = String::from("sha256:");
            for byte in digest {
                write!(result, "{byte:02x}").expect("writing a string cannot fail");
            }
            result
        },
    )
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests;

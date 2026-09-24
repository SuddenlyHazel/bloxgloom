//! Read-only v4 codecs for an explicit offline copy-and-convert command.
//!
//! Normal `Storage` startup never calls these functions and never upgrades a
//! source directory in place. The converter must first recover the legacy WAL
//! and hold the source lock before using these decoded values.

use std::collections::{BTreeMap, HashMap};
use std::io;

use crate::content::{BlockStateId, Catalog, ContentManifest, ItemId};
use crate::world::{CHUNK_VOLUME, TERRAIN_GENERATOR_VERSION};

use super::{SavedEdits, checksum, invalid_data};

const CONTENT_MAGIC: &[u8; 4] = b"BGCM";
const EDIT_MAGIC: &[u8; 4] = b"BGED";
const META_MAGIC: &[u8; 4] = b"BGWD";
const MAX_CONTENT_BYTES: usize = 8 + 512 * (3 + u8::MAX as usize) + 4;
const EDIT_HEADER_LEN: usize = 4 + 2 + 2 + 8 + 8 + 2;
const MAX_EDIT_BYTES: usize = EDIT_HEADER_LEN + CHUNK_VOLUME * 3 + 4;

/// Legacy byte identities resolved to the destination's frozen `u32` IDs.
/// Only the exact v4 builtin catalog is accepted; v1 had no schema hashes with
/// which to prove the meaning of third-party definitions.
#[derive(Clone, Debug)]
pub struct LegacyIdMap {
    states: [Option<BlockStateId>; 256],
    items: [Option<ItemId>; 256],
}

impl LegacyIdMap {
    pub fn state(&self, old: u8) -> Option<BlockStateId> {
        self.states[old as usize]
    }

    pub fn item(&self, old: u8) -> Option<ItemId> {
        self.items[old as usize]
    }

    /// `None` represents a v4 world created before `content.map` was added.
    pub fn from_content_map_v1(bytes: Option<&[u8]>, destination: &Catalog) -> io::Result<Self> {
        destination
            .validate()
            .map_err(|_| invalid_data("incomplete destination catalog"))?;
        let builtins = Catalog::builtins();
        if let Some(bytes) = bytes {
            let saved = decode_content_map_v1(bytes)?;
            let expected = builtins
                .identities()
                .into_iter()
                .filter(|(kind, _, _, _)| matches!(*kind, b'B' | b'I'))
                .map(|(kind, id, key, _)| ((kind, id as u8), key.to_owned()))
                .collect::<BTreeMap<_, _>>();
            if saved != expected {
                return Err(invalid_data(
                    "legacy content map differs from the known builtin catalog",
                ));
            }
        }

        let expected = ContentManifest::from_catalog(&builtins);
        let available = ContentManifest::from_catalog(destination)
            .entries
            .into_iter()
            .map(|entry| {
                (
                    (entry.kind, entry.key),
                    (entry.id, entry.schema_fingerprint),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut states = [None; 256];
        let mut items = [None; 256];
        for entry in expected.entries {
            let Some(&(id, fingerprint)) = available.get(&(entry.kind, entry.key.clone())) else {
                return Err(invalid_data(
                    "destination is missing legacy builtin content",
                ));
            };
            if fingerprint != entry.schema_fingerprint {
                return Err(invalid_data("destination changed legacy builtin content"));
            }
            match entry.kind {
                b'S' if entry.id < 256 => states[entry.id as usize] = Some(BlockStateId(id)),
                b'I' if entry.id < 256 => items[entry.id as usize] = Some(ItemId(id)),
                _ => {}
            }
        }
        Ok(Self { states, items })
    }
}

/// BGWD v4 metadata: magic[4], terrain generator u16, seed u64.
pub fn decode_world_meta_v4(bytes: &[u8]) -> io::Result<u64> {
    if bytes.len() != 14
        || &bytes[..4] != META_MAGIC
        || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != TERRAIN_GENERATOR_VERSION
    {
        return Err(invalid_data("incompatible legacy world metadata"));
    }
    Ok(u64::from_le_bytes(bytes[6..14].try_into().unwrap()))
}

/// BGED v2: magic[4], format u16, terrain generator u16, seed u64,
/// revision u64, count u16, sorted `(cell u16, old block byte)` records,
/// FNV-1a checksum u32. The old byte maps to that block's default state.
pub fn decode_bged_v2(bytes: &[u8], seed: u64, ids: &LegacyIdMap) -> io::Result<SavedEdits> {
    if !(EDIT_HEADER_LEN + 4..=MAX_EDIT_BYTES).contains(&bytes.len())
        || &bytes[..4] != EDIT_MAGIC
        || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != 2
        || u16::from_le_bytes(bytes[6..8].try_into().unwrap()) != TERRAIN_GENERATOR_VERSION
        || u64::from_le_bytes(bytes[8..16].try_into().unwrap()) != seed
    {
        return Err(invalid_data("invalid legacy chunk edits header"));
    }
    let count = u16::from_le_bytes(bytes[24..26].try_into().unwrap()) as usize;
    if count > CHUNK_VOLUME || bytes.len() != EDIT_HEADER_LEN + count * 3 + 4 {
        return Err(invalid_data("invalid legacy chunk edits length"));
    }
    let checksum_at = bytes.len() - 4;
    if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
        != checksum(&bytes[..checksum_at])
    {
        return Err(invalid_data("legacy chunk edits checksum mismatch"));
    }
    let mut blocks = BTreeMap::new();
    let mut previous = None;
    for record in bytes[EDIT_HEADER_LEN..checksum_at].chunks_exact(3) {
        let index = u16::from_le_bytes(record[..2].try_into().unwrap());
        if index as usize >= CHUNK_VOLUME || previous.is_some_and(|last| index <= last) {
            return Err(invalid_data("duplicate or reordered legacy chunk edit"));
        }
        let state = ids
            .state(record[2])
            .ok_or_else(|| invalid_data("unknown legacy block ID"))?;
        blocks.insert(index, state);
        previous = Some(index);
    }
    Ok(SavedEdits {
        version: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        blocks,
    })
}

/// BGCM v1: magic[4], version u16, count u16, sorted `(kind,id,key_len,key)`
/// records, FNV-1a checksum u32. Schema fingerprints did not exist in v1.
fn decode_content_map_v1(bytes: &[u8]) -> io::Result<BTreeMap<(u8, u8), String>> {
    if !(12..=MAX_CONTENT_BYTES).contains(&bytes.len())
        || &bytes[..4] != CONTENT_MAGIC
        || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != 1
    {
        return Err(invalid_data("invalid legacy content map"));
    }
    let count = u16::from_le_bytes(bytes[6..8].try_into().unwrap()) as usize;
    if count > 512 || bytes.len() < 8 + count * 3 + 4 {
        return Err(invalid_data("invalid legacy content count"));
    }
    let checksum_at = bytes.len() - 4;
    if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
        != checksum(&bytes[..checksum_at])
    {
        return Err(invalid_data("legacy content checksum mismatch"));
    }
    let mut entries = BTreeMap::new();
    let mut offset = 8;
    let mut previous = None;
    for _ in 0..count {
        let header = bytes
            .get(offset..offset + 3)
            .filter(|_| offset + 3 <= checksum_at)
            .ok_or_else(|| invalid_data("truncated legacy content entry"))?;
        let (kind, id, len) = (header[0], header[1], header[2] as usize);
        offset += 3;
        if !matches!(kind, b'B' | b'I')
            || previous.is_some_and(|last| (kind, id) <= last)
            || len == 0
        {
            return Err(invalid_data("invalid legacy content identity"));
        }
        let key = std::str::from_utf8(
            bytes
                .get(offset..offset + len)
                .filter(|_| offset + len <= checksum_at)
                .ok_or_else(|| invalid_data("truncated legacy content key"))?,
        )
        .map_err(|_| invalid_data("invalid legacy content key"))?;
        entries.insert((kind, id), key.to_owned());
        offset += len;
        previous = Some((kind, id));
    }
    if offset != checksum_at {
        return Err(invalid_data("trailing legacy content bytes"));
    }
    Ok(entries)
}

#[cfg(test)]
#[path = "legacy/tests.rs"]
mod tests;

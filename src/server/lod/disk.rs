//! Disposable, checksummed LOD data keyed by the exact authoritative inputs.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
const MAGIC: &[u8; 8] = b"BGLOD002";
pub(super) fn path(root: &Path, key: TileKey) -> PathBuf {
    root.join(format!("{}_{}_{}.tile", key.level, key.x, key.z))
}
pub(super) fn load(
    path: &Path,
    stamp: [u8; 32],
    world: &World,
    job: &worker::Job,
) -> Option<LodTile> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take((crate::protocol::MAX_FRAME + 80) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() < 72 || &bytes[..8] != MAGIC || bytes[8..40] != stamp {
        return None;
    }
    let end = bytes.len() - 32;
    if Sha256::digest(&bytes[..end]).as_slice() != &bytes[end..] {
        return None;
    }
    let ServerMessage::LodTile { mut tile, .. } =
        crate::protocol::read_server_with_catalog(&bytes[40..end], world.catalog()).ok()?
    else {
        return None;
    };
    if tile.key != job.key {
        return None;
    }
    tile.revision = job.revision;
    Some(tile)
}
pub(super) fn store(path: &Path, stamp: [u8; 32], world: &World, tile: &LodTile) {
    let mut bytes = Vec::from(MAGIC.as_slice());
    bytes.extend(stamp);
    if crate::protocol::write_server_with_catalog(
        &mut bytes,
        &ServerMessage::LodTile {
            session: 1,
            request: 1,
            tile: tile.clone(),
        },
        world.catalog(),
    )
    .is_err()
    {
        return;
    }
    let checksum = Sha256::digest(&bytes);
    bytes.extend(checksum);
    let temp = path.with_extension("tmp");
    if fs::write(&temp, bytes).is_ok() {
        let _ = fs::rename(&temp, path);
    }
    if let Some(parent) = path.parent() {
        trim(parent);
    }
}
fn trim(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    entries.sort_by_key(|e| e.0);
    let excess = entries.len().saturating_sub(128);
    for (_, path) in entries.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

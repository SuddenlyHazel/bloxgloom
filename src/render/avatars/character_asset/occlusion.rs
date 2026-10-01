//! Offline local visibility, keyed to exact native meshes. No runtime ray casts.
//! Surface semantics stay in the low byte of the existing GPU attribute; the
//! next byte is normalized indirect-light visibility. Native mesh data and
//! source textures remain unmodified.
use sha2::{Digest, Sha256};

const BAKED: &[u8] =
    include_bytes!("../../../../assets/models/player/articulated/local_visibility.ao");
const MIN_VISIBILITY: u8 = 191;

pub(in crate::render::avatars) fn builtin_visibility(vertex_count: usize) -> Vec<u8> {
    let visibility = decode(BAKED, &super::mesh::MESHES)
        .expect("checked-in character local visibility must match native meshes; rerun tools/character_assets/bake_occlusion.py");
    assert_eq!(visibility.len(), vertex_count);
    visibility
}

pub(in crate::render::avatars) fn pack_surface(surface: u32, visibility: u8) -> u32 {
    debug_assert!(surface <= 9);
    surface | (u32::from(visibility) << 8)
}

fn take<'a>(data: &mut &'a [u8], count: usize) -> Result<&'a [u8], String> {
    let (head, tail) = data
        .split_at_checked(count)
        .ok_or("truncated character local visibility")?;
    *data = tail;
    Ok(head)
}

fn decode(mut data: &[u8], meshes: &[&[u8]]) -> Result<Vec<u8>, String> {
    let header = take(&mut data, 8)?;
    if &header[..4] != b"BGA1"
        || meshes.len() != super::MATERIAL_COUNT
        || u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize != meshes.len()
    {
        return Err("invalid character local visibility header".into());
    }
    let mut visibility = Vec::new();
    for &mesh in meshes {
        let header = take(&mut data, 36)?;
        let count = u32::from_le_bytes(header[32..36].try_into().unwrap()) as usize;
        if mesh.len() < 12
            || count == 0
            || count > 16384
            || count != u32::from_le_bytes(mesh[4..8].try_into().unwrap()) as usize
            || visibility.len() + count > 65536
        {
            return Err("invalid character local visibility bounds".into());
        }
        if Sha256::digest(mesh)[..] != header[..32] {
            return Err("stale character local visibility mesh hash".into());
        }
        let values = take(&mut data, count)?;
        if values.iter().any(|&v| v < MIN_VISIBILITY) {
            return Err("character local visibility exceeds attenuation limit".into());
        }
        visibility.extend_from_slice(values);
    }
    if !data.is_empty() {
        return Err("trailing character local visibility data".into());
    }
    Ok(visibility)
}

#[cfg(test)]
mod tests;

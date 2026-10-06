//! Compact completed raw block-light samples for headless precipitation.
use crate::{
    lighting::LightField,
    world::{CHUNK_SIZE, Chunk},
};
pub(crate) fn cache(field: &LightField) -> Box<[u8]> {
    let mut glow = vec![0u8; CHUNK_SIZE.pow(3)];
    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let local = [x, y, z];
                glow[Chunk::index(local).unwrap()] = field.face(local, 0, 0).glow;
            }
        }
    }
    glow.into_boxed_slice()
}
pub(crate) fn sample(
    position: glam::Vec3,
    cache: &std::collections::HashMap<crate::world::ChunkKey, Box<[u8]>>,
) -> f32 {
    let (key, local) = crate::world::world_to_chunk(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    );
    cache
        .get(&key)
        .and_then(|values| Chunk::index(local).and_then(|i| values.get(i)))
        .map_or(0.0, |value| f32::from(*value) / 15.0)
}
#[cfg(test)]
mod tests;

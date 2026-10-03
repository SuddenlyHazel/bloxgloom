//! Direct sky visibility above the local propagation volume. Generated height
//! is not an occluder: cave carving can remove an entire surface column.
//! Edits outside the captured client interest remain unknown; fallback terrain
//! here is lighting context, never an authoritative world snapshot.
use super::*;

pub(super) fn incoming(
    key: ChunkKey,
    known: &HashMap<ChunkKey, Arc<Chunk>>,
    seed: u64,
    catalog: &Catalog,
    blocks: &[BlockId],
) -> Vec<u8> {
    // Fully closed local columns need no generated context above the halo.
    // Partial absorption belongs to the downward pass, exactly once.
    let mut open: Vec<_> = (0..SIDE * SIDE)
        .map(|at| {
            if catalog.sky_attenuation(blocks[index(at % SIDE, SIDE - 1, at / SIDE)]) == MAX_LIGHT {
                0
            } else {
                MAX_LIGHT
            }
        })
        .collect();
    for cz in 0..3 {
        for cx in 0..3 {
            let Some(first) = key_offset(key, cx as i32 - 1, 2, cz as i32 - 1) else {
                continue;
            };
            let last = known
                .keys()
                .filter(|k| k.x == first.x && k.z == first.z)
                .map(|k| k.y)
                .max()
                .unwrap_or(first.y - 1)
                .max(world::MAX_GENERATED_HEIGHT.div_euclid(CHUNK_SIZE as i32));
            for y in first.y..=last {
                if !(0..CHUNK_SIZE).any(|z| {
                    (0..CHUNK_SIZE)
                        .any(|x| open[(cz * CHUNK_SIZE + z) * SIDE + cx * CHUNK_SIZE + x] != 0)
                }) {
                    break;
                }
                let upper_key = ChunkKey { y, ..first };
                let generated;
                let chunk = if let Some(chunk) = known.get(&upper_key) {
                    chunk.as_ref()
                } else {
                    generated = world::generate_chunk(upper_key, seed);
                    &generated
                };
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        let at = (cz * CHUNK_SIZE + z) * SIDE + cx * CHUNK_SIZE + x;
                        if open[at] == 0 {
                            continue;
                        }
                        for y in 0..CHUNK_SIZE {
                            open[at] =
                                open[at].saturating_sub(catalog.sky_attenuation(
                                    chunk.block([x, y, z]).expect("complete chunk"),
                                ));
                            if open[at] == 0 {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    open
}

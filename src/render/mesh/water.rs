//! Greedy fluid boundaries. Only authoritative neighbor snapshots hide a seam.
use super::{Catalog, Chunk, ChunkKey, ChunkMesh, LightField, LightSample, content, world};
use std::{collections::HashMap, sync::Arc};
pub(crate) const FLOATS: usize = 12;
pub(super) fn append(
    out: &mut ChunkMesh,
    chunk: &Chunk,
    light: Option<&LightField>,
    catalog: &Catalog,
    known: &HashMap<ChunkKey, Arc<Chunk>>,
) {
    let n = world::CHUNK_SIZE;
    if !chunk
        .blocks
        .iter()
        .any(|id| catalog.block_flags(*id) & content::FLUID != 0)
    {
        return;
    }
    let origin = [
        i64::from(chunk.key.x) * n as i64,
        i64::from(chunk.key.y) * n as i64,
        i64::from(chunk.key.z) * n as i64,
    ];
    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for side in [-1, 1] {
            let mut mask = vec![0u64; n * n];
            for slice in 0..n {
                mask.fill(0);
                for j in 0..n {
                    for i in 0..n {
                        let mut p = [0; 3];
                        p[axis] = slice;
                        p[u] = i;
                        p[v] = j;
                        let id = chunk.block(p).unwrap_or(world::AIR);
                        if catalog.block_flags(id) & content::FLUID == 0 {
                            continue;
                        }
                        let mut neighbor = p.map(|value| value as i64);
                        neighbor[axis] += side as i64;
                        let adjacent = if neighbor.iter().all(|value| (0..n as i64).contains(value))
                        {
                            Some(
                                chunk
                                    .block(neighbor.map(|value| value as usize))
                                    .unwrap_or(world::AIR),
                            )
                        } else {
                            let xyz = std::array::from_fn::<_, 3, _>(|a| origin[a] + neighbor[a]);
                            match xyz.map(i32::try_from) {
                                [Ok(x), Ok(y), Ok(z)] => {
                                    let (key, local) = world::world_to_chunk(x, y, z);
                                    known.get(&key).and_then(|c| c.block(local))
                                }
                                _ => None,
                            }
                        };
                        if adjacent.is_some_and(|id| {
                            catalog.block_flags(id) & (content::FLUID | content::OPAQUE) != 0
                        }) {
                            continue;
                        }
                        let sample = light.map_or(
                            LightSample {
                                sky: 15,
                                ..Default::default()
                            },
                            |f| f.face(p, axis, side),
                        );
                        mask[i + n * j] = u64::from(id.get())
                            | (u64::from(sample.sky) << 32)
                            | (u64::from(sample.glow) << 36);
                    }
                }
                for j in 0..n {
                    let mut i = 0;
                    while i < n {
                        let material = mask[i + n * j];
                        if material == 0 {
                            i += 1;
                            continue;
                        }
                        let mut width = 1;
                        while i + width < n && mask[i + width + n * j] == material {
                            width += 1;
                        }
                        let mut height = 1;
                        'grow: while j + height < n {
                            for dx in 0..width {
                                if mask[i + dx + n * (j + height)] != material {
                                    break 'grow;
                                }
                            }
                            height += 1;
                        }
                        for dy in 0..height {
                            for dx in 0..width {
                                mask[i + dx + n * (j + dy)] = 0;
                            }
                        }
                        let id = world::BlockId::new(material as u32);
                        let color = catalog.block(id).map_or([0.02, 0.14, 0.28, 0.62], |b| {
                            let mut c = b.swatch;
                            for value in &mut c[..3] {
                                *value = if *value <= 0.04045 {
                                    *value / 12.92
                                } else {
                                    ((*value + 0.055) / 1.055).powf(2.4)
                                };
                            }
                            c
                        });
                        let base = (out.water_vertices.len() / FLOATS) as u32;
                        let mut normal = [0.0; 3];
                        normal[axis] = side as f32;
                        for (du, dv) in [(0, 0), (width, 0), (width, height), (0, height)] {
                            let mut p = origin.map(|value| value as f32);
                            p[axis] += (slice + usize::from(side > 0)) as f32;
                            p[u] += (i + du) as f32;
                            p[v] += (j + dv) as f32;
                            out.water_vertices.extend_from_slice(&p);
                            out.water_vertices.extend_from_slice(&normal);
                            out.water_vertices.extend_from_slice(&color);
                            out.water_vertices.extend_from_slice(&[
                                ((material >> 32) & 15) as f32 / 15.0,
                                ((material >> 36) & 15) as f32 / 15.0,
                            ]);
                        }
                        let order = if side > 0 {
                            [0, 1, 2, 0, 2, 3]
                        } else {
                            [0, 2, 1, 0, 3, 2]
                        };
                        out.water_indices.extend(order.map(|index| base + index));
                        i += width;
                    }
                }
            }
        }
    }
}
#[cfg(test)]
mod tests;

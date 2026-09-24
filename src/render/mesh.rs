use crate::lighting::{LightField, LightSample};
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey};

use super::VERTEX_FLOATS;
use super::material::{face_uv, material_layer};

/// Interleaved position, normal, tiled UV, and texture layer. World-space
/// coordinates avoid per-draw uniforms; one material bind group serves all chunks.
pub struct ChunkMesh {
    pub key: ChunkKey,
    pub version: u64,
    pub(crate) lighting_revision: u64,
    pub(crate) vertices: Vec<f32>,
    pub(crate) indices: Vec<u32>,
}

impl ChunkMesh {
    pub(crate) fn byte_len(&self) -> usize {
        self.vertices.len() * 4 + self.indices.len() * 4
    }

    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}
pub(super) struct GpuMesh {
    pub(super) lighting_revision: u64,
    pub(super) vertex: wgpu::Buffer,
    pub(super) index: wgpu::Buffer,
    pub(super) indices: u32,
}

/// Greedy mesh opaque blocks, merging coplanar faces with the same block ID.
/// Uses the shared world model's x, z, y indexing; 0 is air.
#[cfg(test)]
pub fn mesh_chunk(chunk: &Chunk) -> ChunkMesh {
    mesh_chunk_with_light(chunk, None, 0)
}

pub fn mesh_chunk_lit(chunk: &Chunk, light: &LightField, lighting_revision: u64) -> ChunkMesh {
    mesh_chunk_with_light(chunk, Some(light), lighting_revision)
}

fn mesh_chunk_with_light(
    chunk: &Chunk,
    light: Option<&LightField>,
    lighting_revision: u64,
) -> ChunkMesh {
    let n = CHUNK_SIZE;
    let mut out = ChunkMesh {
        key: chunk.key,
        version: chunk.version,
        lighting_revision,
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    if chunk.blocks.len() != n * n * n {
        return out;
    }
    let origin = [
        chunk.key.x as f32 * n as f32,
        chunk.key.y as f32 * n as f32,
        chunk.key.z as f32 * n as f32,
    ];
    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for side in [-1i32, 1] {
            let mut mask = vec![0u32; n * n];
            for slice in 0..n {
                mask.fill(0);
                for j in 0..n {
                    for i in 0..n {
                        let mut p = [0usize; 3];
                        p[axis] = slice;
                        p[u] = i;
                        p[v] = j;
                        let block = block_at(chunk, p, n);
                        if block == 0 {
                            continue;
                        }
                        let edge = if side > 0 { slice + 1 == n } else { slice == 0 };
                        let exposed = if edge {
                            true
                        } else {
                            let mut adjacent = p;
                            adjacent[axis] = (slice as i32 + side) as usize;
                            block_at(chunk, adjacent, n) == 0
                        };
                        if exposed {
                            let sample = light.map_or(
                                LightSample {
                                    sky: 15,
                                    glow: 0,
                                    bounce: [0; 3],
                                },
                                |field| field.face(p, axis, side),
                            );
                            // Bounce luminance limits greedy merging so localized
                            // reflected light survives large flat surfaces.
                            let bounce_level =
                                sample.bounce.iter().copied().max().unwrap_or(0) / 16;
                            mask[i + n * j] = u32::from(block)
                                | (u32::from(sample.sky) << 8)
                                | (u32::from(sample.glow) << 12)
                                | (u32::from(bounce_level) << 16);
                        }
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
                        emit_quad(
                            &mut out, origin, axis, u, v, side, slice, i, j, width, height,
                            material, light,
                        );
                        i += width;
                    }
                }
            }
        }
    }
    out
}

fn block_at(chunk: &Chunk, p: [usize; 3], n: usize) -> u8 {
    chunk.blocks[p[0] + n * (p[2] + n * p[1])]
}

#[allow(clippy::too_many_arguments)]
fn emit_quad(
    out: &mut ChunkMesh,
    origin: [f32; 3],
    axis: usize,
    u: usize,
    v: usize,
    side: i32,
    slice: usize,
    i: usize,
    j: usize,
    width: usize,
    height: usize,
    material: u32,
    light: Option<&LightField>,
) {
    let base = (out.vertices.len() / VERTEX_FLOATS) as u32;
    let mut normal = [0.0; 3];
    normal[axis] = side as f32;
    let layer = material_layer((material & 255) as u8, axis, side);
    let sky = f32::from(((material >> 8) & 15) as u8) / 15.0;
    let glow = f32::from(((material >> 12) & 15) as u8) / 15.0;
    let corners = [(0, 0), (width, 0), (width, height), (0, height)];
    for (du, dv) in corners {
        let mut position = origin;
        position[axis] += (slice + usize::from(side > 0)) as f32;
        position[u] += (i + du) as f32;
        position[v] += (j + dv) as f32;
        out.vertices.extend_from_slice(&position);
        out.vertices.extend_from_slice(&normal);
        let (texture_u, texture_v) =
            face_uv(axis, du as f32, dv as f32, width as f32, height as f32);
        let corner_light = light.map_or([sky, glow, 0.0, 0.0, 0.0], |field| {
            field.corner([axis, u, v], side, slice, [i + du, j + dv])
        });
        // Every 24-bit integer is represented exactly by f32. Packing RGB
        // keeps the default path only one float wider than its old vertex.
        let packed_bounce = (corner_light[2] * 255.0).round() as u32
            | (((corner_light[3] * 255.0).round() as u32) << 8)
            | (((corner_light[4] * 255.0).round() as u32) << 16);
        out.vertices.extend_from_slice(&[
            texture_u,
            texture_v,
            layer as f32,
            corner_light[0],
            corner_light[1],
            packed_bounce as f32,
        ]);
    }
    // (u, v, axis) is cyclic for every axis, so +axis is CCW.
    if side > 0 {
        out.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        out.indices
            .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

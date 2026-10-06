use crate::content::{self, Catalog};
use crate::lighting::{LightField, LightSample};
use crate::world::{self, BlockId, CHUNK_SIZE, Chunk, ChunkKey};

use super::VERTEX_FLOATS;
use super::material::face_uv;

mod foliage;
mod sources;
pub(crate) mod water;

/// Interleaved position, normal, tiled UV, and texture layer. World-space
/// coordinates avoid per-draw uniforms; one material bind group serves all chunks.
pub struct ChunkMesh {
    pub key: ChunkKey,
    pub version: u64,
    pub(crate) lighting_revision: u64,
    pub(crate) local_sources: Vec<super::local_shadow::Source>,
    pub(crate) trace: std::sync::Arc<super::trace::scene::Chunk>,
    pub(crate) vertices: Vec<f32>,
    pub(crate) indices: Vec<u32>,
    pub(crate) cutout_vertices: Vec<f32>,
    pub(crate) cutout_indices: Vec<u32>,
    pub(crate) water_vertices: Vec<f32>,
    pub(crate) water_indices: Vec<u32>,
}

impl ChunkMesh {
    pub(crate) fn draw_byte_len(&self) -> usize {
        (self.vertices.len()
            + self.indices.len()
            + self.cutout_vertices.len()
            + self.cutout_indices.len()
            + self.water_vertices.len()
            + self.water_indices.len())
            * 4
    }

    /// Full worker payload used to bound admission, including retained ray geometry.
    pub(crate) fn byte_len(&self) -> usize {
        self.draw_byte_len()
            + self.local_sources.len() * std::mem::size_of::<super::local_shadow::Source>()
            + self.trace.byte_len()
    }

    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        (self.indices.len() + self.cutout_indices.len() + self.water_indices.len()) / 3
    }
}
pub(super) struct GpuMesh {
    pub(super) lighting_revision: u64,
    pub(super) local_sources: Vec<super::local_shadow::Source>,
    pub(super) opaque: Option<GpuSubmesh>,
    pub(super) cutout: Option<GpuSubmesh>,
    pub(super) water: Option<GpuSubmesh>,
}

pub(super) struct GpuSubmesh {
    pub(super) vertex: wgpu::Buffer,
    pub(super) index: wgpu::Buffer,
    pub(super) indices: u32,
}

/// Greedy mesh opaque blocks, merging coplanar faces with the same block ID.
/// Uses the shared world model's x, z, y indexing; 0 is air.
#[cfg(test)]
pub fn mesh_chunk(chunk: &Chunk) -> ChunkMesh {
    mesh_chunk_with_catalog(chunk, None, 0, content::catalog(), &Default::default())
}

#[cfg(test)]
pub fn mesh_chunk_lit_with_catalog(
    chunk: &Chunk,
    light: &LightField,
    lighting_revision: u64,
    catalog: &Catalog,
) -> ChunkMesh {
    mesh_chunk_lit_with_neighbors(
        chunk,
        light,
        lighting_revision,
        catalog,
        &Default::default(),
    )
}

pub fn mesh_chunk_lit_with_neighbors(
    chunk: &Chunk,
    light: &LightField,
    lighting_revision: u64,
    catalog: &Catalog,
    known: &std::collections::HashMap<ChunkKey, std::sync::Arc<Chunk>>,
) -> ChunkMesh {
    mesh_chunk_with_catalog(chunk, Some(light), lighting_revision, catalog, known)
}

fn mesh_chunk_with_catalog(
    chunk: &Chunk,
    light: Option<&LightField>,
    lighting_revision: u64,
    catalog: &Catalog,
    known: &std::collections::HashMap<ChunkKey, std::sync::Arc<Chunk>>,
) -> ChunkMesh {
    let n = CHUNK_SIZE;
    let mut out = ChunkMesh {
        key: chunk.key,
        version: chunk.version,
        lighting_revision,
        local_sources: Vec::new(),
        trace: Default::default(),
        vertices: Vec::new(),
        indices: Vec::new(),
        cutout_vertices: Vec::new(),
        cutout_indices: Vec::new(),
        water_vertices: Vec::new(),
        water_indices: Vec::new(),
    };
    if chunk.blocks.len() != n * n * n {
        return out;
    }
    let Some(resolved) = ResolvedChunk::new(chunk, catalog) else {
        return out;
    };
    let origin = [
        chunk.key.x as f32 * n as f32,
        chunk.key.y as f32 * n as f32,
        chunk.key.z as f32 * n as f32,
    ];
    out.local_sources = sources::collect(&resolved, origin, catalog);
    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for side in [-1i32, 1] {
            // Full 32-bit state identity plus light quantization: no u8 ID
            // truncation in the greedy merge key.
            let mut mask = vec![0u64; n * n];
            for slice in 0..n {
                mask.fill(0);
                for j in 0..n {
                    for i in 0..n {
                        let mut p = [0usize; 3];
                        p[axis] = slice;
                        p[u] = i;
                        p[v] = j;
                        let block = resolved.block_at(p, n);
                        if !block.has(content::OPAQUE) {
                            continue;
                        }
                        let edge = if side > 0 { slice + 1 == n } else { slice == 0 };
                        let exposed = if edge {
                            true
                        } else {
                            let mut adjacent = p;
                            adjacent[axis] = (slice as i32 + side) as usize;
                            !resolved.block_at(adjacent, n).has(content::OPAQUE)
                        };
                        if exposed {
                            let sample = light.map_or(
                                LightSample {
                                    sky: 15,
                                    glow: 0,
                                    bounce: [0; 3],
                                    glow_bounce: [0; 3],
                                    ..LightSample::default()
                                },
                                |field| field.face(p, axis, side),
                            );
                            // Bounce luminance limits greedy merging so localized
                            // reflected light survives large flat surfaces.
                            let bounce_level =
                                sample.bounce.iter().copied().max().unwrap_or(0) / 16;
                            // Keep lit faces cell-sized: scalar ties can hide color or
                            // incoming-direction gradients inside a greedy rectangle.
                            mask[i + n * j] = u64::from(block.id.get())
                                | (u64::from(sample.sky) << 32)
                                | (u64::from(sample.glow) << 36)
                                | (u64::from(bounce_level) << 40)
                                | (u64::from(
                                    sample.glow_bounce.iter().copied().max().unwrap_or(0) / 16,
                                ) << 44)
                                | (if sample.glow > 0 {
                                    (i + n * j + 1) as u64
                                } else {
                                    0
                                } << 48);
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
                        let mut face_cell = [0usize; 3];
                        face_cell[axis] = slice;
                        face_cell[u] = i;
                        face_cell[v] = j;
                        emit_quad(
                            &mut out.vertices,
                            &mut out.indices,
                            origin,
                            axis,
                            u,
                            v,
                            side,
                            slice,
                            i,
                            j,
                            width,
                            height,
                            material,
                            resolved.block_at(face_cell, n).face_layer(axis, side),
                            light,
                        );
                        i += width;
                    }
                }
            }
        }
    }
    for y in 0..n {
        for z in 0..n {
            for x in 0..n {
                let p = [x, y, z];
                let block = resolved.block_at(p, n);
                if block.botanical && block.has(content::CUTOUT) && !block.has(content::PLANT) {
                    let exposed = foliage::leaf_exposure(p, chunk.key, &resolved, catalog, known);
                    if exposed > 0 {
                        foliage::emit_leaf_cluster(&mut out, origin, p, block, exposed, light);
                    }
                } else if block.has(content::CUTOUT) && !block.has(content::PLANT) {
                    for axis in 0..3 {
                        let u = (axis + 1) % 3;
                        let v = (axis + 2) % 3;
                        for side in [-1, 1] {
                            let adjacent = p[axis] as i32 + side;
                            let visible = if (0..n as i32).contains(&adjacent) {
                                let mut neighbor = p;
                                neighbor[axis] = adjacent as usize;
                                let neighbor_block = resolved.block_at(neighbor, n);
                                neighbor_block.id != block.id
                                    && !neighbor_block.has(content::OPAQUE)
                            } else {
                                true
                            };
                            if visible {
                                let sample = light.map_or(
                                    LightSample {
                                        sky: 15,
                                        glow: 0,
                                        bounce: [0; 3],
                                        glow_bounce: [0; 3],
                                        ..LightSample::default()
                                    },
                                    |field| field.face(p, axis, side),
                                );
                                let bounce = sample.bounce.iter().copied().max().unwrap_or(0) / 16;
                                let material = u64::from(block.id.get())
                                    | (u64::from(sample.sky) << 32)
                                    | (u64::from(sample.glow) << 36)
                                    | (u64::from(bounce) << 40)
                                    | (u64::from(
                                        sample.glow_bounce.iter().copied().max().unwrap_or(0) / 16,
                                    ) << 44);
                                emit_quad(
                                    &mut out.cutout_vertices,
                                    &mut out.cutout_indices,
                                    origin,
                                    axis,
                                    u,
                                    v,
                                    side,
                                    p[axis],
                                    p[u],
                                    p[v],
                                    1,
                                    1,
                                    material,
                                    block.face_layer(axis, side),
                                    light,
                                );
                            }
                        }
                    }
                } else if block.has(content::PLANT) {
                    emit_plant(
                        &mut out,
                        origin,
                        p,
                        block.face_layer(1, 1),
                        block.plant_upper,
                        light,
                    );
                }
            }
        }
    }
    water::append(&mut out, chunk, light, catalog, known);
    out.trace = std::sync::Arc::new(super::trace::scene::Chunk::from_world_mesh(
        &out, catalog, chunk,
    ));
    out
}

#[derive(Clone, Copy)]
struct ResolvedBlock {
    id: BlockId,
    flags: u8,
    face_layers: [u32; 6],
    botanical: bool,
    plant_upper: bool,
    timber: bool,
}

impl ResolvedBlock {
    fn new(id: BlockId, catalog: &Catalog) -> Self {
        let state = catalog.state(id);
        let mut face_layers = [3; 6];
        if let Some(state) = state {
            for axis in 0..3 {
                for side in [-1, 1] {
                    let face = axis * 2 + usize::from(side > 0);
                    face_layers[face] = state
                        .face_texture(axis, side)
                        .map_or(3, |texture| texture.get());
                }
            }
        }
        Self {
            id,
            flags: catalog.block_flags(id),
            face_layers,
            plant_upper: state.is_some_and(|s| {
                s.properties
                    .iter()
                    .any(|(name, value)| name == "half" && value == "upper")
            }),
            timber: state.is_some_and(|s| {
                let key = s.key.split('[').next().unwrap_or(&s.key);
                key.ends_with(":wood") || key.ends_with("_log") || key.ends_with("_wood")
            }),
            botanical: catalog
                .texture(crate::content::TextureId::new(face_layers[3]))
                .is_some_and(|t| {
                    t.alpha_cutout
                        && t.key.starts_with("bloxgloom:")
                        && (t.foliage.wrap > 0.0 || t.foliage.transmission > 0.0)
                }),
        }
    }

    #[inline]
    fn has(self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    #[inline]
    fn face_layer(self, axis: usize, side: i32) -> u32 {
        self.face_layers[axis * 2 + usize::from(side > 0)]
    }
}

/// Resolves the chunk's state palette once, then uses compact local indices
/// during meshing instead of catalog lookups in the voxel and face loops.
struct ResolvedChunk {
    palette: Vec<ResolvedBlock>,
    indices: Vec<u16>,
}

impl ResolvedChunk {
    fn new(chunk: &Chunk, catalog: &Catalog) -> Option<Self> {
        let mut palette = Vec::new();
        let mut indices = Vec::with_capacity(chunk.blocks.len());
        match chunk.blocks.view() {
            world::PaletteView::Uniform(state) => {
                palette.push(ResolvedBlock::new(state, catalog));
                indices.resize(chunk.blocks.len(), 0);
            }
            world::PaletteView::Palette8 {
                palette: states,
                indices: local,
            } => {
                palette.extend(
                    states
                        .iter()
                        .copied()
                        .map(|state| ResolvedBlock::new(state, catalog)),
                );
                indices.extend(local.iter().copied().map(u16::from));
            }
            world::PaletteView::Palette16 {
                palette: states,
                indices: local,
            } => {
                palette.extend(
                    states
                        .iter()
                        .copied()
                        .map(|state| ResolvedBlock::new(state, catalog)),
                );
                indices.extend_from_slice(local);
            }
            world::PaletteView::InvalidLength(_) => return None,
        }
        Some(Self { palette, indices })
    }

    #[inline]
    fn block_at(&self, p: [usize; 3], n: usize) -> ResolvedBlock {
        self.palette[self.indices[p[0] + n * (p[2] + n * p[1])] as usize]
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_quad(
    vertices: &mut Vec<f32>,
    indices: &mut Vec<u32>,
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
    material: u64,
    layer: u32,
    light: Option<&LightField>,
) {
    let base = (vertices.len() / VERTEX_FLOATS) as u32;
    let mut normal = [0.0; 3];
    normal[axis] = side as f32;
    let sky = f32::from(((material >> 32) & 15) as u8) / 15.0;
    let glow = f32::from(((material >> 36) & 15) as u8) / 15.0;
    let corners = [(0, 0), (width, 0), (width, height), (0, height)];
    for (du, dv) in corners {
        let mut position = origin;
        position[axis] += (slice + usize::from(side > 0)) as f32;
        position[u] += (i + du) as f32;
        position[v] += (j + dv) as f32;
        vertices.extend_from_slice(&position);
        vertices.extend_from_slice(&normal);
        let (texture_u, texture_v) =
            face_uv(axis, du as f32, dv as f32, width as f32, height as f32);
        let (corner_light, visibility) = light
            .map_or(([sky, glow, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1.0), |field| {
                field.corner_with_visibility([axis, u, v], side, slice, [i + du, j + dv])
            });
        let (local_rgb, local_direction) = light.map_or(([0.0; 3], [0.0; 3]), |field| {
            field.corner_local([axis, u, v], side, slice, [i + du, j + dv])
        });
        // Every 24-bit integer is represented exactly by f32. Packing RGB
        // preserves exact packed bytes without reinterpretation on the CPU.
        let packed_bounce = (corner_light[2] * 255.0).round() as u32
            | (((corner_light[3] * 255.0).round() as u32) << 8)
            | (((corner_light[4] * 255.0).round() as u32) << 16);
        let packed_glow_bounce = (corner_light[5] * 255.0).round() as u32
            | (((corner_light[6] * 255.0).round() as u32) << 8)
            | (((corner_light[7] * 255.0).round() as u32) << 16);
        vertices.extend_from_slice(&[
            texture_u,
            texture_v,
            // Integer layer identity is unchanged; the otherwise unused
            // fractional half encodes baked corner visibility without a wider
            // vertex. Decode before interpolation, never after flat layer cast.
            layer as f32 + (1.0 - visibility) * 0.5,
            corner_light[0],
            corner_light[1],
            packed_bounce as f32,
            packed_glow_bounce as f32,
            pack_light_rgb(&local_rgb) as f32,
            pack_light_direction(local_direction) as f32,
        ]);
    }
    // (u, v, axis) is cyclic for every axis, so +axis is CCW.
    if side > 0 {
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

fn emit_plant(
    out: &mut ChunkMesh,
    origin: [f32; 3],
    p: [usize; 3],
    layer: u32,
    upper: bool,
    light: Option<&LightField>,
) {
    let layer = layer as f32;
    let world = [
        origin[0] + p[0] as f32,
        origin[1] + p[1] as f32,
        origin[2] + p[2] as f32,
    ];
    // Crossed diagonals give each plant a visible silhouette from every angle.
    // One winding suffices because the cutout pipeline renders both sides.
    for (start, end) in [([0.08, 0.08], [0.92, 0.92]), ([0.08, 0.92], [0.92, 0.08])] {
        let base = (out.cutout_vertices.len() / VERTEX_FLOATS) as u32;
        for (t, height, uv) in [
            (0.0, 0.02, [0.0, 1.0]),
            (1.0, 0.02, [1.0, 1.0]),
            (1.0, 0.98, [1.0, 0.0]),
            (0.0, 0.98, [0.0, 0.0]),
        ] {
            let [x, z] = foliage::plant_position(
                [world[0], world[1] - if upper { 1.0 } else { 0.0 }, world[2]],
                start[0] + (end[0] - start[0]) * t,
                start[1] + (end[1] - start[1]) * t,
            );
            let (sample, visibility) =
                light.map_or(([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1.0), |field| {
                    field.spatial_with_visibility([
                        p[0] as f32 + x,
                        p[1] as f32 + height,
                        p[2] as f32 + z,
                    ])
                });
            let (local_rgb, local_direction) = light.map_or(([0.0; 3], [0.0; 3]), |field| {
                field.spatial_local([p[0] as f32 + x, p[1] as f32 + height, p[2] as f32 + z])
            });
            let packed_bounce = pack_light_rgb(&sample[2..5]);
            let packed_glow_bounce = pack_light_rgb(&sample[5..8]);
            out.cutout_vertices.extend_from_slice(&[
                world[0] + x,
                world[1] + height,
                world[2] + z,
                0.0,
                1.0,
                0.0,
                uv[0],
                uv[1],
                layer + (1.0 - visibility) * 0.5,
                sample[0],
                sample[1],
                packed_bounce as f32,
                packed_glow_bounce as f32,
                pack_light_rgb(&local_rgb) as f32,
                pack_light_direction(local_direction) as f32,
            ]);
        }
        out.cutout_indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn pack_light_rgb(rgb: &[f32]) -> u32 {
    (rgb[0] * 255.0).round() as u32
        | (((rgb[1] * 255.0).round() as u32) << 8)
        | (((rgb[2] * 255.0).round() as u32) << 16)
}

/// Three signed normalized bytes fit exactly in the 24-bit integer mantissa.
pub(super) fn pack_light_direction(direction: [f32; 3]) -> u32 {
    let bytes = direction.map(|v| (v.clamp(-1.0, 1.0) * 127.0).round() as i8 as u8);
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
}

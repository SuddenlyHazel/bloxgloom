//! Actual fluid interfaces and authoritative occupancy, independent of shell closure.
use super::{Triangle, surface};
use crate::{content, render::ChunkMesh, world};

/// Selected server-provided coarse medium presentation. Unknown vertical gaps
/// remain unknown; these intervals describe the admitted LOD representation,
/// never unseen fine world voxels or procedural client guesses.
#[derive(Clone, Debug)]
pub(crate) struct CoarseColumn {
    pub coverage: Vec<crate::lod::Interval>,
    pub water: Vec<crate::lod::Interval>,
}

#[derive(Clone, Debug)]
pub(crate) struct CoarseTile {
    pub key: crate::lod::TileKey,
    pub columns: Vec<CoarseColumn>,
}

impl CoarseTile {
    pub(crate) fn from_lod(tile: &crate::lod::LodTile, catalog: &content::Catalog) -> Option<Self> {
        tile.validate(catalog).ok()?;
        let columns = tile
            .columns
            .iter()
            .map(|column| {
                let mut water: Vec<crate::lod::Interval> = Vec::new();
                for span in &column.spans {
                    if catalog.block_flags(span.state) & content::FLUID == 0 {
                        continue;
                    }
                    if let Some(last) = water.last_mut()
                        && last.top == span.bottom
                    {
                        last.top = span.top;
                    } else {
                        water.push(crate::lod::Interval {
                            bottom: span.bottom,
                            top: span.top,
                        });
                    }
                }
                CoarseColumn {
                    coverage: column.coverage.clone(),
                    water,
                }
            })
            .collect();
        Some(Self {
            key: tile.key,
            columns,
        })
    }

    /// Packed directory: key/width header, four offset/count words per column,
    /// then exact lower/upper pairs. Root scene packing charges its own header.
    pub(crate) fn byte_len(&self) -> usize {
        32 + self.columns.len() * 16
            + self
                .columns
                .iter()
                .map(|column| (column.coverage.len() + column.water.len()) * 8)
                .sum::<usize>()
    }
}

/// Only fluid-bearing chunks carry this payload. Loaded dry chunks are known
/// through scene coverage; absence outside that coverage must remain unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Occupancy {
    /// One means entirely water; two means the exact mixed-voxel mask follows.
    pub class: u32,
    pub mask: Vec<u32>,
}

impl Occupancy {
    pub(crate) fn from_chunk(chunk: &world::Chunk, catalog: &content::Catalog) -> Option<Self> {
        if chunk.blocks.len() != world::CHUNK_VOLUME {
            return None;
        }
        let mut mask = vec![0u32; world::CHUNK_VOLUME / 32];
        let mut count = 0;
        for (index, id) in chunk.blocks.iter().enumerate() {
            if catalog.block_flags(*id) & content::FLUID != 0 {
                mask[index / 32] |= 1 << (index % 32);
                count += 1;
            }
        }
        match count {
            0 => None,
            world::CHUNK_VOLUME => Some(Self {
                class: 1,
                mask: Vec::new(),
            }),
            _ => Some(Self { class: 2, mask }),
        }
    }

    pub(crate) fn byte_len(&self) -> usize {
        4 + self.mask.len() * 4
    }
}

/// Preserve the raster mesher's real exposed boundaries, including its
/// deliberate omission of unknown lateral residency walls. Volume membership
/// comes from Occupancy above, never from assuming these triangles are closed.
pub(crate) fn append(mesh: &ChunkMesh, _catalog: &content::Catalog, triangles: &mut Vec<Triangle>) {
    let stride = crate::render::mesh::water::FLOATS;
    triangles.reserve(mesh.water_indices.len() / 3);
    for indices in mesh.water_indices.chunks_exact(3) {
        let vertices = [indices[0], indices[1], indices[2]]
            .map(|i| &mesh.water_vertices[i as usize * stride..][..stride]);
        let v = vertices[0];
        triangles.push(
            Triangle {
                a: [v[0], v[1], v[2], 0.0],
                b: [vertices[1][0], vertices[1][1], vertices[1][2], v[10]],
                c: [vertices[2][0], vertices[2][1], vertices[2][2], 0.0],
                uv_ab: [0.0; 4],
                uv_c: [0.0; 2],
                surface_color: 0,
                surface_flags: 0,
                // This is the outward fluid normal, even for back-facing hits.
                normal: [v[3], v[4], v[5], -1.0],
            }
            .with_surface(
                [v[6], v[7], v[8], v[9]],
                surface::WATER | surface::COARSE_COLOR | surface::NO_WIND,
            ),
        );
    }
}

#[cfg(test)]
mod tests;

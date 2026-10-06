//! Immutable, stackless triangle BVH. Build/rebuild only on mesh/scene workers.
use super::super::{ChunkMesh, VERTEX_FLOATS};
use glam::Vec3;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Triangle {
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub c: [f32; 4],
    pub uv_ab: [f32; 4],
    pub uv_c: [f32; 4],
    pub normal: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Node {
    pub min: [f32; 3],
    pub first: u32,
    pub max: [f32; 3],
    pub count: u32,
    pub escape: u32,
    pub padding: [u32; 3],
}
#[derive(Default, Debug)]
pub(crate) struct Chunk {
    pub key: Option<crate::world::ChunkKey>,
    pub triangles: Vec<Triangle>,
}
#[derive(Default, Debug)]
pub(crate) struct Scene {
    pub triangles: Vec<Triangle>,
    pub nodes: Vec<Node>,
    pub coverage: Vec<u32>,
}
impl Chunk {
    pub fn from_mesh(mesh: &ChunkMesh, catalog: &crate::content::Catalog) -> Self {
        let mut triangles =
            Vec::with_capacity((mesh.indices.len() + mesh.cutout_indices.len()) / 3);
        for (vertices, indices, cutout) in [
            (&mesh.vertices, &mesh.indices, false),
            (&mesh.cutout_vertices, &mesh.cutout_indices, true),
        ] {
            for indices in indices.chunks_exact(3) {
                let v = [indices[0], indices[1], indices[2]]
                    .map(|i| &vertices[i as usize * VERTEX_FLOATS..][..VERTEX_FLOATS]);
                let stationary =
                    bounds::stationary(catalog.textures().get(v[0][8].floor() as usize));
                let normal = [
                    v[0][3],
                    v[0][4],
                    v[0][5],
                    if stationary { -1.0 } else { 0.0 },
                ];
                triangles.push(Triangle {
                    a: [v[0][0], v[0][1], v[0][2], v[0][8].floor()],
                    b: [v[1][0], v[1][1], v[1][2], v[0][9]],
                    c: [v[2][0], v[2][1], v[2][2], if cutout { 1.0 } else { 0.0 }],
                    uv_ab: [v[0][6], v[0][7], v[1][6], v[1][7]],
                    uv_c: [v[2][6], v[2][7], 0.0, 0.0],
                    normal,
                });
            }
        }
        Self {
            key: Some(mesh.key),
            triangles,
        }
    }
    pub fn byte_len(&self) -> usize {
        self.triangles.len() * std::mem::size_of::<Triangle>()
    }
}
impl Scene {
    pub fn fits(&self, limit: u64) -> bool {
        (self.triangles.len() * std::mem::size_of::<Triangle>()) as u64 <= limit
            && (self.nodes.len() * std::mem::size_of::<Node>()) as u64 <= limit
            && (self.coverage.len() * 4) as u64 <= limit
    }
    pub fn build(chunks: impl IntoIterator<Item = std::sync::Arc<Chunk>>) -> Self {
        Self::build_with_bounds(chunks, super::optimizations::tight_bounds())
    }
    pub(super) fn build_with_bounds(
        chunks: impl IntoIterator<Item = std::sync::Arc<Chunk>>,
        tight: bool,
    ) -> Self {
        let chunks: Vec<_> = chunks.into_iter().collect();
        let mut result = Self {
            coverage: coverage::build(chunks.iter().filter_map(|c| c.key)),
            ..Default::default()
        };
        for chunk in chunks {
            result.triangles.extend_from_slice(&chunk.triangles);
        }
        if !result.triangles.is_empty() {
            result.partition(0, result.triangles.len(), tight);
        }
        result
    }
    fn partition(&mut self, first: usize, count: usize, tight: bool) {
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for triangle in &self.triangles[first..first + count] {
            for p in [triangle.a, triangle.b, triangle.c] {
                let position = Vec3::new(p[0], p[1], p[2]);
                let padding = Vec3::splat(bounds::padding(triangle, tight));
                low = low.min(position - padding);
                high = high.max(position + padding);
            }
        }
        // Unknown/botanical triangles retain the full .12m allowance; only
        // worker-classified stationary builtin triangles use numerical padding.
        let index = self.nodes.len();
        self.nodes.push(Node {
            min: low.to_array(),
            max: high.to_array(),
            first: first as u32,
            count: count as u32,
            escape: 0,
            padding: [0; 3],
        });
        if count > 8 {
            let split = bvh::split(&mut self.triangles[first..first + count], tight);
            self.nodes[index].count = 0;
            self.partition(first, split, tight);
            self.partition(first + split, count - split, tight);
        }
        self.nodes[index].escape = self.nodes.len() as u32;
    }
    pub fn byte_len(&self) -> usize {
        self.triangles.len() * std::mem::size_of::<Triangle>()
            + self.nodes.len() * std::mem::size_of::<Node>()
            + self.coverage.len() * 4
    }
}

mod bvh;
mod coverage;
#[cfg(test)]
mod tests;

mod bounds;

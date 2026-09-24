//! Small textured block meshes for nearby dropped items.
use super::{VERTEX_FLOATS, material::material_layer};
use crate::protocol::DroppedItem;

pub(super) const MAX_ITEMS: usize = 256;
pub(super) const MAX_VERTEX_BYTES: u64 = (MAX_ITEMS * 24 * VERTEX_FLOATS * 4) as u64;
pub(super) const MAX_INDEX_BYTES: u64 = (MAX_ITEMS * 36 * 4) as u64;

pub(crate) fn mesh(items: &[DroppedItem]) -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::with_capacity(items.len().min(MAX_ITEMS) * 24 * VERTEX_FLOATS);
    let mut indices = Vec::with_capacity(items.len().min(MAX_ITEMS) * 36);
    for item in items.iter().take(MAX_ITEMS) {
        let center = item.position;
        for axis in 0..3 {
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            for side in [-1i32, 1] {
                let base = (vertices.len() / VERTEX_FLOATS) as u32;
                let mut normal = [0.0; 3];
                normal[axis] = side as f32;
                for (du, dv) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                    let mut position = center;
                    position[axis] += 0.20 * side as f32;
                    position[u] += (du - 0.5) * 0.40;
                    position[v] += (dv - 0.5) * 0.40;
                    vertices.extend(position);
                    vertices.extend(normal);
                    vertices.extend([
                        du,
                        dv,
                        material_layer(item.block, axis, side) as f32,
                        0.72,
                        0.0,
                        0.0,
                    ]);
                }
                if side > 0 {
                    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                } else {
                    indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
                }
            }
        }
    }
    (vertices, indices)
}

//! Small textured block meshes for nearby dropped items.
use super::{VERTEX_FLOATS, material::material_layer};
use glam::Vec3;

pub(super) const MAX_ITEMS: usize = 512;
pub(super) const MAX_VERTEX_BYTES: u64 = (MAX_ITEMS * 24 * VERTEX_FLOATS * 4) as u64;
pub(super) const MAX_INDEX_BYTES: u64 = (MAX_ITEMS * 36 * 4) as u64;

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualDrop {
    pub block: u8,
    pub center: Vec3,
    pub angle: f32,
    pub scale: f32,
}

pub(crate) fn mesh(items: &[VisualDrop]) -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::with_capacity(items.len().min(MAX_ITEMS) * 24 * VERTEX_FLOATS);
    let mut indices = Vec::with_capacity(items.len().min(MAX_ITEMS) * 36);
    for item in items.iter().take(MAX_ITEMS) {
        let (sin, cos) = item.angle.sin_cos();
        for axis in 0..3 {
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            for side in [-1i32, 1] {
                let base = (vertices.len() / VERTEX_FLOATS) as u32;
                let mut normal = [0.0; 3];
                normal[axis] = side as f32;
                for (du, dv) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                    let mut local = [0.0; 3];
                    local[axis] += 0.23 * side as f32;
                    local[u] += (du - 0.5) * 0.46;
                    local[v] += (dv - 0.5) * 0.46;
                    let local = Vec3::from_array(local) * item.scale;
                    let position = item.center
                        + Vec3::new(
                            local.x * cos + local.z * sin,
                            local.y,
                            -local.x * sin + local.z * cos,
                        );
                    let normal = Vec3::from_array(normal);
                    let normal = Vec3::new(
                        normal.x * cos + normal.z * sin,
                        normal.y,
                        -normal.x * sin + normal.z * cos,
                    );
                    vertices.extend(position.to_array());
                    vertices.extend(normal.to_array());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotated_drop_stays_bounded_and_uses_all_six_faces() {
        let drop = VisualDrop {
            block: 2,
            center: Vec3::new(10.0, 5.0, -2.0),
            angle: 0.7,
            scale: 1.0,
        };
        let (vertices, indices) = mesh(&[drop]);
        assert_eq!(vertices.len(), 24 * VERTEX_FLOATS);
        assert_eq!(indices.len(), 36);
        for vertex in vertices.chunks_exact(VERTEX_FLOATS) {
            let position = Vec3::new(vertex[0], vertex[1], vertex[2]);
            assert!(position.distance(drop.center) < 0.41);
        }
    }
}

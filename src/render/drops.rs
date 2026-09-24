//! Small textured block meshes for nearby dropped items.
use super::{
    VERTEX_FLOATS,
    material::{face_uv, item_material_layer},
};
use crate::world::is_plant;
use glam::Vec3;

pub(super) const MAX_ITEMS: usize = 512;
pub(super) const MAX_VERTEX_BYTES: u64 = (MAX_ITEMS * 24 * VERTEX_FLOATS * 4) as u64;
pub(super) const MAX_INDEX_BYTES: u64 = (MAX_ITEMS * 36 * 4) as u64;
pub(super) const MAX_CUTOUT_VERTEX_BYTES: u64 = (MAX_ITEMS * 8 * VERTEX_FLOATS * 4) as u64;
pub(super) const MAX_CUTOUT_INDEX_BYTES: u64 = (MAX_ITEMS * 12 * 4) as u64;

pub(crate) struct DropMeshes {
    pub opaque_vertices: Vec<f32>,
    pub opaque_indices: Vec<u32>,
    pub cutout_vertices: Vec<f32>,
    pub cutout_indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualDrop {
    pub item: u8,
    pub center: Vec3,
    pub angle: f32,
    pub scale: f32,
}

fn is_sprite_item(item: u8) -> bool {
    crate::content::item_def(item).is_some_and(|definition| definition.sprite)
}

pub(crate) fn mesh(items: &[VisualDrop]) -> DropMeshes {
    let (cutout_count, opaque_count) =
        items
            .iter()
            .take(MAX_ITEMS)
            .fold((0, 0), |(cutout, opaque), item| {
                if is_sprite_item(item.item) {
                    (cutout + 1, opaque)
                } else {
                    (cutout, opaque + 1)
                }
            });
    let mut vertices = Vec::with_capacity(opaque_count * 24 * VERTEX_FLOATS);
    let mut indices = Vec::with_capacity(opaque_count * 36);
    let mut cutout_vertices = Vec::with_capacity(cutout_count * 8 * VERTEX_FLOATS);
    let mut cutout_indices = Vec::with_capacity(cutout_count * 12);
    for item in items.iter().take(MAX_ITEMS) {
        if is_sprite_item(item.item) {
            emit_cutout_drop(item, &mut cutout_vertices, &mut cutout_indices);
            continue;
        }
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
                    let (texture_u, texture_v) = face_uv(axis, du, dv, 1.0, 1.0);
                    vertices.extend([
                        texture_u,
                        texture_v,
                        item_material_layer(item.item, axis, side) as f32,
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
    DropMeshes {
        opaque_vertices: vertices,
        opaque_indices: indices,
        cutout_vertices,
        cutout_indices,
    }
}

fn emit_cutout_drop(item: &VisualDrop, vertices: &mut Vec<f32>, indices: &mut Vec<u32>) {
    let (sin, cos) = item.angle.sin_cos();
    let layer = item_material_layer(item.item, 1, 1) as f32;
    let half_size = if crate::content::item_def(item.item)
        .and_then(|definition| definition.placeable)
        .is_some_and(is_plant)
    {
        0.32
    } else {
        0.23
    };
    for (start, end) in [
        ([-half_size, -half_size], [half_size, half_size]),
        ([-half_size, half_size], [half_size, -half_size]),
    ] {
        let base = (vertices.len() / VERTEX_FLOATS) as u32;
        for (t, height, u, v) in [
            (0.0, -half_size, 0.0, 1.0),
            (1.0, -half_size, 1.0, 1.0),
            (1.0, half_size, 1.0, 0.0),
            (0.0, half_size, 0.0, 0.0),
        ] {
            let x = (start[0] + (end[0] - start[0]) * t) * item.scale;
            let z = (start[1] + (end[1] - start[1]) * t) * item.scale;
            let position =
                item.center + Vec3::new(x * cos + z * sin, height * item.scale, -x * sin + z * cos);
            vertices.extend_from_slice(&[
                position.x, position.y, position.z, 0.0, 1.0, 0.0, u, v, layer, 0.72, 0.0, 0.0,
            ]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::SEEDS;
    use crate::world::GRASS;

    #[test]
    fn rotated_drop_stays_bounded_and_uses_all_six_faces() {
        let drop = VisualDrop {
            item: 2,
            center: Vec3::new(10.0, 5.0, -2.0),
            angle: 0.7,
            scale: 1.0,
        };
        let mesh = mesh(&[drop]);
        assert_eq!(mesh.opaque_vertices.len(), 24 * VERTEX_FLOATS);
        assert_eq!(mesh.opaque_indices.len(), 36);
        for vertex in mesh.opaque_vertices.chunks_exact(VERTEX_FLOATS) {
            let position = Vec3::new(vertex[0], vertex[1], vertex[2]);
            assert!(position.distance(drop.center) < 0.41);
        }
    }

    #[test]
    fn grass_side_band_is_at_the_top_on_both_side_axes() {
        let vertices = mesh(&[VisualDrop {
            item: GRASS,
            center: Vec3::ZERO,
            angle: 0.0,
            scale: 1.0,
        }])
        .opaque_vertices;
        for face in [0, 1, 4, 5] {
            for vertex in vertices[face * 4 * VERTEX_FLOATS..(face + 1) * 4 * VERTEX_FLOATS]
                .chunks_exact(VERTEX_FLOATS)
            {
                assert_eq!(vertex[8], 1.0, "side must use grass-side texture");
                assert_eq!(vertex[7], if vertex[1] > 0.0 { 0.0 } else { 1.0 });
            }
        }
    }

    #[test]
    fn flower_pickup_uses_cutout_crosses_instead_of_cube_faces() {
        let mesh = mesh(&[VisualDrop {
            item: crate::world::RED_FLOWER,
            center: Vec3::new(4.0, 2.0, -1.0),
            angle: 0.35,
            scale: 1.0,
        }]);
        assert!(mesh.opaque_indices.is_empty());
        assert_eq!(mesh.cutout_vertices.len(), 8 * VERTEX_FLOATS);
        assert_eq!(mesh.cutout_indices.len(), 12);
        assert!(
            mesh.cutout_vertices
                .chunks_exact(VERTEX_FLOATS)
                .all(|vertex| vertex[8] == 12.0)
        );
    }

    #[test]
    fn seed_pickup_uses_item_artwork() {
        let mesh = mesh(&[VisualDrop {
            item: SEEDS,
            center: Vec3::ZERO,
            angle: 0.0,
            scale: 1.0,
        }]);
        assert!(mesh.opaque_indices.is_empty());
        assert_eq!(mesh.cutout_indices.len(), 12);
        assert!(
            mesh.cutout_vertices
                .chunks_exact(VERTEX_FLOATS)
                .all(|vertex| vertex[8] == 17.0)
        );
    }
}

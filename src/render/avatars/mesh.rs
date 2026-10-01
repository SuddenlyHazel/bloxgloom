use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct AvatarVertex {
    pub(super) position: [f32; 3],
    pub(super) normal: [f32; 3],
    pub(super) part: u32,
    pub(super) color: [f32; 3],
}

pub(super) struct AvatarMesh {
    pub(super) vertices: Vec<AvatarVertex>,
    pub(super) indices: Vec<u32>,
}

pub(super) fn emit_cuboid(mesh: &mut AvatarMesh, min: [f32; 3], max: [f32; 3], part: u32) {
    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for side in [-1i32, 1] {
            let base = mesh.vertices.len() as u32;
            let mut normal = [0.0; 3];
            normal[axis] = side as f32;
            for (du, dv) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                let mut position = [0.0; 3];
                position[axis] = if side < 0 { min[axis] } else { max[axis] };
                position[u] = if du == 0 { min[u] } else { max[u] };
                position[v] = if dv == 0 { min[v] } else { max[v] };
                mesh.vertices.push(AvatarVertex {
                    position,
                    normal,
                    part,
                    color: [1.0; 3],
                });
            }
            if side > 0 {
                mesh.indices
                    .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            } else {
                mesh.indices
                    .extend([base, base + 2, base + 1, base, base + 3, base + 2]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_cuboid_mesh_has_bounded_closed_geometry() {
        let mut mesh = AvatarMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        emit_cuboid(&mut mesh, [-0.2, 0.0, -0.2], [0.2, 0.5, 0.2], 5);
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
        assert!(
            mesh.indices
                .iter()
                .all(|index| (*index as usize) < mesh.vertices.len())
        );
        assert!(mesh.vertices.iter().all(|vertex| vertex.part == 5));
        assert!(mesh.vertices.iter().all(|vertex| {
            vertex.position.iter().all(|axis| axis.is_finite())
                && vertex
                    .normal
                    .iter()
                    .filter(|axis| axis.abs() == 1.0)
                    .count()
                    == 1
        }));
    }
}

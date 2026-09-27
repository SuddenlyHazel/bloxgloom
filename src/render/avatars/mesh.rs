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

pub(super) fn build() -> AvatarMesh {
    let mut mesh = AvatarMesh {
        vertices: Vec::with_capacity(11 * 24),
        indices: Vec::with_capacity(11 * 36),
    };
    // A readable low-poly silhouette with no per-avatar geometry rebuild.
    // Part IDs select registered public cosmetic bytes in the GPU shader.
    for (min, max, part) in [
        ([-0.23, 1.33, -0.23], [0.23, 1.77, 0.23], 0),   // face
        ([-0.28, 0.68, -0.15], [0.28, 1.33, 0.15], 1),   // shirt
        ([-0.43, 0.79, -0.14], [-0.29, 1.29, 0.14], 1),  // left sleeve
        ([0.29, 0.79, -0.14], [0.43, 1.29, 0.14], 1),    // right sleeve
        ([-0.43, 0.64, -0.13], [-0.29, 0.79, 0.13], 0),  // left hand
        ([0.29, 0.64, -0.13], [0.43, 0.79, 0.13], 0),    // right hand
        ([-0.23, 0.0, -0.13], [-0.02, 0.68, 0.13], 2),   // left leg
        ([0.02, 0.0, -0.13], [0.23, 0.68, 0.13], 2),     // right leg
        ([-0.24, 1.70, -0.24], [0.24, 1.80, 0.24], 3),   // hair
        ([-0.13, 1.52, 0.233], [-0.06, 1.58, 0.243], 4), // eyes
        ([0.06, 1.52, 0.233], [0.13, 1.58, 0.243], 4),
    ] {
        emit_cuboid(&mut mesh, min, max, part);
    }
    mesh
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
    fn humanoid_mesh_has_bounded_closed_cuboids_and_face_details() {
        let mesh = build();
        assert_eq!(mesh.vertices.len(), 11 * 24);
        assert_eq!(mesh.indices.len(), 11 * 36);
        assert!(
            mesh.indices
                .iter()
                .all(|index| (*index as usize) < mesh.vertices.len())
        );
        assert!(mesh.vertices.iter().any(|vertex| vertex.part == 4));
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

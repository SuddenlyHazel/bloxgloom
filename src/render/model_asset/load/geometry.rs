//! Static and four-weight skinned triangles share the same palette layout.
use super::*;

pub(super) fn load(
    g: &gltf::Gltf,
    blob: &[u8],
    order: &[Option<usize>],
    node_count: usize,
    material_count: usize,
) -> Result<(Vec<Binding>, Vec<Primitive>)> {
    let mut bindings: Vec<_> = (0..node_count)
        .map(|node| Binding {
            node,
            inverse_bind: Mat4::IDENTITY,
        })
        .collect();
    let mut skins = vec![Vec::new(); g.skins().len()];
    for skin in g.skins() {
        let joints: Vec<_> = skin.joints().collect();
        ensure(
            !joints.is_empty() && joints.len() <= 256 && bindings.len() + joints.len() <= 1024,
            "skin joint palette limit exceeded",
        )?;
        let reader = skin.reader(|_| Some(blob));
        let matrices: Vec<_> = reader
            .read_inverse_bind_matrices()
            .map(|v| v.map(|m| Mat4::from_cols_array_2d(&m)).collect())
            .unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
        ensure(
            matrices.len() == joints.len(),
            "skin inverse bind count mismatch",
        )?;
        for (joint, inverse_bind) in joints.into_iter().zip(matrices) {
            let node = order[joint.index()].ok_or("skin joint outside active scene")?;
            ensure(
                inverse_bind.is_finite() && inverse_bind.determinant().abs() > 1e-12,
                "invalid inverse bind matrix",
            )?;
            skins[skin.index()].push(bindings.len() as u32);
            bindings.push(Binding { node, inverse_bind });
        }
    }
    let mut primitives = Vec::new();
    let (mut vertex_total, mut index_total) = (0, 0);
    for node in g.nodes().filter(|n| order[n.index()].is_some()) {
        let Some(mesh) = node.mesh() else { continue };
        let attached = order[node.index()].unwrap();
        for primitive in mesh.primitives() {
            ensure(
                primitive.mode() == gltf::mesh::Mode::Triangles
                    && primitive.morph_targets().next().is_none(),
                "only triangles supported; morph targets are not supported yet",
            )?;
            let material = primitive
                .material()
                .index()
                .ok_or("primitive needs a named material")?;
            ensure(
                material < material_count && primitives.len() < 1024,
                "primitive material or count invalid",
            )?;
            let reader = primitive.reader(|_| Some(blob));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or("primitive has no positions")?
                .collect();
            let normals: Vec<_> = reader
                .read_normals()
                .ok_or("export vertex normals with the GLB")?
                .collect();
            let uvs: Vec<_> = reader
                .read_tex_coords(0)
                .map(|v| v.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0; 2]; positions.len()]);
            ensure(
                !positions.is_empty()
                    && positions.len() == normals.len()
                    && positions.len() == uvs.len(),
                "primitive attribute length mismatch",
            )?;
            let mut joints = vec![[attached as u32; 4]; positions.len()];
            let mut weights = vec![[1.0, 0.0, 0.0, 0.0]; positions.len()];
            if let Some(skin) = node.skin() {
                ensure(
                    reader.read_joints(1).is_none() && reader.read_weights(1).is_none(),
                    "more than four skin weights unsupported",
                )?;
                joints = reader
                    .read_joints(0)
                    .ok_or("skin has no vertex joints")?
                    .into_u16()
                    .map(|v| v.map(u32::from))
                    .collect();
                weights = reader
                    .read_weights(0)
                    .ok_or("skin has no vertex weights")?
                    .into_f32()
                    .collect();
                ensure(
                    joints.len() == positions.len() && weights.len() == positions.len(),
                    "skin attribute length mismatch",
                )?;
                let palette = &skins[skin.index()];
                for (js, ws) in joints.iter_mut().zip(&mut weights) {
                    let sum: f32 = ws.iter().sum();
                    ensure(
                        ws.iter().all(|w| w.is_finite() && *w >= 0.0) && (sum - 1.0).abs() <= 0.01,
                        "skin weights must sum to one",
                    )?;
                    for i in 0..4 {
                        js[i] = if ws[i] == 0.0 {
                            palette[0]
                        } else {
                            *palette
                                .get(js[i] as usize)
                                .ok_or("vertex skin joint out of bounds")?
                        };
                        ws[i] /= sum;
                    }
                }
            }
            let indices: Vec<_> = reader
                .read_indices()
                .map(|v| v.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            vertex_total += positions.len();
            index_total += indices.len();
            ensure(
                vertex_total <= 65536
                    && index_total <= 196608
                    && !indices.is_empty()
                    && indices.len().is_multiple_of(3)
                    && indices.iter().all(|&i| i < positions.len() as u32),
                "model mesh budget or indices invalid",
            )?;
            let mut vertices = Vec::with_capacity(positions.len());
            for i in 0..positions.len() {
                let position = Vec3::from_array(positions[i]);
                let normal = Vec3::from_array(normals[i]);
                ensure(
                    position.is_finite()
                        && position.abs().max_element() <= 256.0
                        && normal.is_finite()
                        && normal.length_squared() > 1e-12
                        && uvs[i].iter().all(|v| v.is_finite()),
                    "nonfinite or invalid model vertex",
                )?;
                vertices.push(Vertex {
                    position: positions[i],
                    normal: normal.normalize().to_array(),
                    uv: uvs[i],
                    joints: joints[i],
                    weights: weights[i],
                    tint: primitives.len() as u32,
                });
            }
            primitives.push(Primitive {
                node: attached,
                material,
                vertices,
                indices,
            });
        }
    }
    ensure(!primitives.is_empty(), "GLB has no active geometry")?;
    Ok((bindings, primitives))
}

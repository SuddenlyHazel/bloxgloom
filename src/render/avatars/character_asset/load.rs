//! Compile the builtin rigid GLB once; no offline files or procedural clips.
use super::*;
use crate::render::model_asset::{Binding, Controls, Look, Node};

pub(super) fn load() -> Result<CharacterAsset, String> {
    let controls: Controls = serde_json::from_slice(include_bytes!(
        "../../../../assets/models/player/master/controls.json"
    ))
    .map_err(|e| e.to_string())?;
    let mut model = Model::from_glb(
        include_bytes!("../../../../assets/models/player/master/model.glb"),
        controls,
    )?;
    let rest = model.sample(None, 0.0)?;
    let source: Vec<_> = rig::NAMES
        .iter()
        .map(|name| {
            model
                .nodes
                .iter()
                .position(|n| n.name == *name)
                .ok_or_else(|| format!("missing player joint {name}"))
        })
        .collect::<Result<_, _>>()?;
    let mut order = vec![None; model.nodes.len()];
    for (i, &node) in source.iter().enumerate() {
        order[node] = Some(i);
    }
    // Every animated node must survive; folding is restricted to static cubes.
    for clip in &mut model.clips {
        for c in &mut clip.channels {
            c.node = order[c.node].ok_or("player animates an unsupported joint")?;
        }
    }
    let mut roles = vec![2; model.primitives.len()];
    for tint in &model.controls.tints {
        let role = match tint.name.as_str() {
            "hair_color" => 7,
            "skin_color" => 0,
            "shorts_color" => 1,
            "shirt_color" => 9,
            "iris_color" => 4,
            _ => return Err("unknown player color control".into()),
        };
        for p in tint.targets(&model)? {
            roles[p] = role;
        }
    }
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for group in 0..MATERIAL_COUNT {
        let hair = if (1..14).contains(&group) {
            crate::appearance::HAIR[group]
        } else {
            "none"
        };
        let body = if group == 14 {
            "defined_chest"
        } else {
            "flat_chest"
        };
        let look: Look =
            serde_json::from_value(serde_json::json!({"variants":{"body":body,"hair_style":hair}}))
                .map_err(|e| e.to_string())?;
        let visible = model.appearance(&look)?.visible;
        for (p_id, p) in model
            .primitives
            .iter()
            .enumerate()
            .filter(|(_, p)| visible[p.node])
        {
            let material = &model.materials[p.material];
            let texture = material
                .texture
                .ok_or("player material needs an embedded texture")?;
            let is_hair = texture != 0;
            if is_hair != (1..14).contains(&group) {
                continue;
            }
            if material.color != [1.0; 4]
                || material.alpha_cutoff != Some(0.05)
                || !material.double_sided
            {
                return Err("player material contract changed".into());
            }
            let mut node = p.node;
            while order[node].is_none() {
                node = model.nodes[node].parent.ok_or("cube has no actor joint")?;
            }
            let joint = order[node].unwrap();
            let delta = rest[node].inverse() * rest[p.node];
            let normals = delta.inverse().transpose();
            let first = vertices.len() as u32;
            for v in &p.vertices {
                if v.weights != [1.0, 0.0, 0.0, 0.0]
                    || model.bindings[v.joints[0] as usize].node != p.node
                {
                    return Err("builtin player must use rigid node geometry".into());
                }
                vertices.push(CharacterVertex {
                    position: delta
                        .transform_point3(Vec3::from_array(v.position))
                        .to_array(),
                    normal: normals
                        .transform_vector3(Vec3::from_array(v.normal))
                        .normalize()
                        .to_array(),
                    uv: v.uv,
                    joint,
                    material: group as u32,
                    surface: if is_hair && roles[p_id] != 7 {
                        8
                    } else {
                        roles[p_id]
                    },
                    texture: texture as u32,
                });
            }
            indices.extend(p.indices.iter().map(|i| first + i));
        }
    }
    let nodes: Vec<_> = source
        .iter()
        .map(|&node| {
            let n = &model.nodes[node];
            Ok(Node {
                name: n.name.clone(),
                rest: n.rest,
                parent: n
                    .parent
                    .map(|p| order[p].ok_or("joint has a static parent"))
                    .transpose()?,
            })
        })
        .collect::<Result<_, String>>()?;
    #[cfg(test)]
    let joints = nodes
        .iter()
        .map(|n| Joint {
            name: n.name.clone(),
            parent: n.parent,
        })
        .collect();
    model.nodes = nodes;
    model.bindings = (0..JOINT_COUNT)
        .map(|node| Binding {
            node,
            inverse_bind: Mat4::IDENTITY,
        })
        .collect();
    model.primitives.clear();
    let images = std::mem::take(&mut model.images);
    let soles = vertices
        .iter()
        .filter(|v| {
            v.material == 0
                && [
                    rig::RIGHT_LEG[2],
                    rig::RIGHT_LEG[3],
                    rig::LEFT_LEG[2],
                    rig::LEFT_LEG[3],
                ]
                .contains(&v.joint)
        })
        .map(|v| (v.joint, Vec3::from_array(v.position)))
        .collect();
    Ok(CharacterAsset {
        #[cfg(test)]
        joints,
        vertices,
        indices,
        animation: model,
        images,
        soles,
    })
}

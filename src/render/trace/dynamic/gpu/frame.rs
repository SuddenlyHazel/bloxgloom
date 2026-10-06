//! Pose/look serialization and immutable-topology admission, without CPU skinning.
use super::*;
pub(super) struct Frame {
    pub words: Vec<u32>,
    pub header: Vec<u32>,
    pub nodes: Vec<u32>,
    pub topology: Vec<usize>,
    pub levels: u32,
    pub triangles: usize,
    pub node_count: usize,
    pub node_base: usize,
    pub topology_changed: bool,
}
pub(super) fn pack(
    targets: &DynamicTargets,
    offsets: &HashMap<usize, u32>,
    previous_topology: &[usize],
    limit: u64,
) -> Option<Frame> {
    let topology: Vec<_> = targets
        .instances
        .iter()
        .map(|i| Arc::as_ptr(&i.asset) as usize)
        .collect();
    let topology_changed = topology != previous_topology;
    let mut frame = vec![0u32; 8 + targets.instances.len() * INSTANCE_WORDS];
    let mut geometry = vec![0u32; 8];
    let mut topology_nodes = Vec::new();
    let mut tlas = Vec::new();
    if !targets.instances.is_empty() {
        tlas_nodes(&mut tlas, 0, targets.instances.len(), 0);
    }
    let tlas_count = tlas.len();
    let triangle_count = targets
        .instances
        .iter()
        .map(|i| i.asset.triangles.len())
        .sum::<usize>();
    let node_count = tlas_count
        + targets
            .instances
            .iter()
            .map(|i| i.asset.nodes.len())
            .sum::<usize>();
    let node_base = 8 + triangle_count * TRIANGLE_WORDS;
    if (node_base + node_count * NODE_WORDS) as u64 * 4 > limit {
        return None;
    }
    if topology_changed {
        topology_nodes.resize(node_count * NODE_WORDS, 0);
    }
    frame[..8].copy_from_slice(&[
        targets.instances.len() as u32,
        triangle_count as u32,
        node_count as u32,
        tlas_count as u32,
        node_base as u32,
        8,
        0,
        0,
    ]);
    geometry[..8].copy_from_slice(&frame[..8]);
    let tlas_levels = tlas.iter().map(|n| n.level + 1).max().unwrap_or(0);
    let mut tri_start = 0;
    let mut node_start = tlas_count;
    let mut levels = tlas_levels;
    for (index, instance) in targets.instances.iter().enumerate() {
        let offset = 8 + index * INSTANCE_WORDS;
        let matrix_offset = frame.len();
        for joint in &instance.joints {
            frame.extend(joint.to_cols_array().map(f32::to_bits));
        }
        let parts_offset = frame.len();
        for (color, visible) in &instance.parts {
            frame.extend(color.map(f32::to_bits));
            frame.extend([u32::from(*visible), 0, 0, 0]);
        }
        let mode = match instance.deformation {
            Deformation::Primitive => 0,
            Deformation::Character => 1,
            Deformation::Authored => 2,
            Deformation::Rigid => 3,
        };
        frame[offset..offset + 8].copy_from_slice(&[
            offsets[&(Arc::as_ptr(&instance.asset) as usize)],
            tri_start as u32,
            node_start as u32,
            instance.asset.triangles.len() as u32,
            instance.asset.nodes.len() as u32,
            matrix_offset as u32,
            parts_offset as u32,
            mode,
        ]);
        frame[offset + 8..offset + 24]
            .copy_from_slice(&instance.world.to_cols_array().map(f32::to_bits));
        frame[offset + 24..offset + 28].copy_from_slice(&instance.pose.map(f32::to_bits));
        frame[offset + 28..offset + 32].copy_from_slice(&[
            instance.tint.x.to_bits(),
            instance.tint.y.to_bits(),
            instance.tint.z.to_bits(),
            instance.sky.to_bits(),
        ]);
        let recipe = instance.recipe;
        frame[offset + 32..offset + 36].copy_from_slice(&[
            u32::from_le_bytes(instance.cosmetics),
            u32::from_le_bytes([recipe.eyes, recipe.mouth, recipe.hair, recipe.body]),
            recipe
                .iris
                .map_or(0, |rgb| u32::from_le_bytes([rgb[0], rgb[1], rgb[2], 1])),
            u32::from_le_bytes([
                recipe.hair_color[0],
                recipe.hair_color[1],
                recipe.hair_color[2],
                0,
            ]),
        ]);
        frame[offset + 36..offset + 40].copy_from_slice(&instance.orientation.map(f32::to_bits));
        frame[offset + 40] = u32::from(instance.skip_primary);
        if topology_changed {
            for (index, node) in instance.asset.nodes.iter().enumerate() {
                let at = (node_start + index) * NODE_WORDS;
                topology_nodes[at + 3] = tri_start as u32 + node.first;
                topology_nodes[at + 7] = node.count;
                topology_nodes[at + 8] = node_start as u32 + node.escape;
                topology_nodes[at + 9] = node_start as u32 + node.right;
                topology_nodes[at + 10] = tlas_levels + node.level;
                topology_nodes[at + 11] = 0;
            }
        }
        node_start += instance.asset.nodes.len();
        tri_start += instance.asset.triangles.len();
        levels = levels.max(tlas_levels + instance.asset.levels);
    }
    for (index, node) in tlas.iter().enumerate() {
        if !topology_changed {
            break;
        }
        let at = index * NODE_WORDS;
        topology_nodes[at + 3] = node.first;
        topology_nodes[at + 7] = node.count;
        topology_nodes[at + 8] = node.escape;
        topology_nodes[at + 9] = node.right;
        topology_nodes[at + 10] = node.level;
        topology_nodes[at + 11] = 1;
    }
    if [&frame, &geometry, &topology_nodes]
        .iter()
        .any(|v| v.len() as u64 * 4 > limit)
        || (node_base + node_count * NODE_WORDS) as u64 * 4 > limit
        || levels > 64
    {
        return None;
    }

    Some(Frame {
        words: frame,
        header: geometry,
        nodes: topology_nodes,
        topology,
        levels,
        triangles: triangle_count,
        node_count,
        node_base,
        topology_changed,
    })
}

struct TopNode {
    first: u32,
    count: u32,
    escape: u32,
    right: u32,
    level: u32,
}
fn tlas_nodes(nodes: &mut Vec<TopNode>, first: usize, count: usize, depth: u32) -> usize {
    let index = nodes.len();
    nodes.push(TopNode {
        first: first as u32,
        count: count as u32,
        escape: 0,
        right: 0,
        level: depth,
    });
    if count > 1 {
        let half = count / 2;
        tlas_nodes(nodes, first, half, depth + 1);
        let right = tlas_nodes(nodes, first + half, count - half, depth + 1);
        nodes[index].count = 0;
        nodes[index].right = right as u32;
    }
    nodes[index].escape = nodes.len() as u32;
    index
}

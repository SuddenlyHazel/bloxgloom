//! Dynamic packets protect real geometry admission and independent frame ownership.
use super::*;

fn vertex(position: [f32; 3]) -> Vertex {
    Vertex {
        position,
        normal: [0.0, 0.0, 1.0],
        uv: [0.0; 2],
        joints: [0; 4],
        weights: [0.0; 4],
        part: 0,
    }
}
#[test]
fn worker_blas_preserves_all_native_triangles_and_contiguous_escape_ranges() {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for i in 0..33 {
        let base = vertices.len() as u32;
        let x = i as f32;
        vertices.extend([
            vertex([x, 0.0, 0.0]),
            vertex([x + 0.5, 0.0, 0.0]),
            vertex([x, 0.5, 0.0]),
        ]);
        triangles.push(([base, base + 1, base + 2], 0, i));
    }
    let asset = DynamicAsset::build(
        vertices,
        triangles,
        vec![Material::flat([0.7, 0.2, 0.1])],
        Vec::new(),
    );
    assert_eq!(asset.triangles.len(), 33);
    assert_eq!(asset.nodes[0].escape as usize, asset.nodes.len());
    let mut seen = vec![false; 33];
    for (i, node) in asset.nodes.iter().enumerate() {
        assert!(node.escape as usize > i && node.escape as usize <= asset.nodes.len());
        if node.count == 0 {
            assert_eq!(asset.nodes[i + 1].escape, node.right);
            assert_eq!(asset.nodes[node.right as usize].escape, node.escape);
        } else {
            assert!(node.count <= 8);
            for t in &asset.triangles[node.first as usize..(node.first + node.count) as usize] {
                assert!(!seen[t.part as usize]);
                seen[t.part as usize] = true;
            }
        }
    }
    assert!(seen.into_iter().all(|v| v));
}
#[test]
fn empty_and_replacement_snapshots_do_not_retain_departed_targets() {
    let asset = DynamicAsset::build(
        vec![
            vertex([0.0; 3]),
            vertex([1.0, 0.0, 0.0]),
            vertex([0.0, 1.0, 0.0]),
        ],
        vec![([0, 1, 2], 0, 0)],
        vec![Material::flat([1.0; 3])],
        Vec::new(),
    );
    let first = DynamicTargets {
        instances: vec![DynamicInstance::rigid(asset.clone(), Mat4::IDENTITY, 1.0)],
    };
    let mut merged = DynamicTargets::default();
    merged.append(&first);
    assert_eq!(merged.instances.len(), 1);
    merged.clear();
    assert!(merged.instances.is_empty());
    merged.append(&DynamicTargets {
        instances: vec![DynamicInstance::rigid(
            asset,
            Mat4::from_translation(Vec3::X * 7.0),
            0.0,
        )],
    });
    assert_eq!(merged.instances[0].world.w_axis.x, 7.0);
    assert_eq!(merged.instances[0].sky, 0.0);
}

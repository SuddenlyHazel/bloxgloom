use super::scene::*;
use std::sync::Arc;
fn triangle(x: f32) -> Triangle {
    Triangle {
        a: [x, 0.0, 0.0, 0.0],
        b: [x + 1.0, 0.0, 0.0, 1.0],
        c: [x, 1.0, 0.0, 0.0],
        uv_ab: [0.0; 4],
        uv_c: [0.0; 4],
        normal: [0.0, 0.0, 1.0, 0.0],
    }
}
#[test]
fn scene_bvh_covers_every_leaf_and_preserves_wind_bounds() {
    let scene = Scene::build([Arc::new(Chunk {
        key: None,
        triangles: (0..128).rev().map(|x| triangle(x as f32)).collect(),
    })]);
    assert_eq!(scene.triangles.len(), 128);
    assert_eq!(scene.nodes[0].escape as usize, scene.nodes.len());
    let mut covered = [false; 128];
    for (i, node) in scene.nodes.iter().enumerate() {
        assert!(node.escape as usize > i && node.escape as usize <= scene.nodes.len());
        if node.count == 0 {
            continue;
        }
        for t in node.first..node.first + node.count {
            assert!(!covered[t as usize]);
            covered[t as usize] = true;
            for p in [
                scene.triangles[t as usize].a,
                scene.triangles[t as usize].b,
                scene.triangles[t as usize].c,
            ] {
                for (axis, value) in p.iter().enumerate().take(3) {
                    assert!(*value >= node.min[axis] + 0.1199 && *value <= node.max[axis] - 0.1199);
                }
            }
        }
    }
    assert!(covered.iter().all(|v| *v));
    let empty = Scene::build([]);
    assert!(empty.nodes.is_empty());
}

mod denoise;
mod transport;

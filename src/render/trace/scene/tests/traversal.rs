//! CPU-only candidate audit. Production BVH layout/traversal remain untouched.
use super::*;
#[derive(Default)]
struct Statistics {
    boxes: usize,
    triangles: usize,
    peak: usize,
    overflow: bool,
}
fn entry(
    scene: &Scene,
    index: usize,
    origin: Vec3,
    inverse: Vec3,
    limit: f32,
    stats: &mut Statistics,
) -> Option<f32> {
    stats.boxes += 1;
    let node = &scene.nodes[index];
    let a = (Vec3::from(node.min) - origin) * inverse;
    let b = (Vec3::from(node.max) - origin) * inverse;
    let near = a.min(b).max_element().max(0.0001);
    let far = a.max(b).min_element().min(limit);
    (near <= far).then_some(near)
}
fn nearest(
    scene: &Scene,
    origin: Vec3,
    direction: Vec3,
    reject_cutouts: bool,
    capacity: usize,
) -> (f32, Statistics) {
    let mut distance = 512f32;
    let mut stats = Statistics::default();
    if scene.nodes.is_empty() {
        return (distance, stats);
    }
    let inverse = Vec3::from_array(
        direction
            .to_array()
            .map(|v| 1. / if v.abs() < 0.0000001 { 0.0000001 } else { v }),
    );
    let Some(root) = entry(scene, 0, origin, inverse, distance, &mut stats) else {
        return (distance, stats);
    };
    assert!(capacity <= 24);
    let mut pending = [(0usize, 0f32); 24];
    let mut pending_len = 0;
    let mut current = Some((0, root));
    while let Some((index, near)) = current {
        if near <= distance {
            let node = &scene.nodes[index];
            if node.count == 0 {
                let left = index + 1;
                let right = scene.nodes[left].escape as usize;
                let children = [
                    (
                        left,
                        entry(scene, left, origin, inverse, distance, &mut stats),
                    ),
                    (
                        right,
                        entry(scene, right, origin, inverse, distance, &mut stats),
                    ),
                ];
                match (children[0].1, children[1].1) {
                    (Some(a), Some(b)) => {
                        let (first, second) = if a <= b {
                            ((left, a), (right, b))
                        } else {
                            ((right, b), (left, a))
                        };
                        if pending_len == capacity {
                            // Correctness fallback restarts existing stackless traversal.
                            // Nothing about a full stack may discard a possible hit.
                            let baseline = cast(scene, origin, direction, reject_cutouts);
                            stats.boxes += baseline.1.nodes;
                            stats.triangles += baseline.1.candidates;
                            stats.overflow = true;
                            return (baseline.0, stats);
                        }
                        pending[pending_len] = second;
                        pending_len += 1;
                        stats.peak = stats.peak.max(pending_len);
                        current = Some(first);
                        continue;
                    }
                    (Some(a), None) => {
                        current = Some((left, a));
                        continue;
                    }
                    (None, Some(b)) => {
                        current = Some((right, b));
                        continue;
                    }
                    (None, None) => {}
                }
            } else {
                for t in &scene.triangles[node.first as usize..(node.first + node.count) as usize] {
                    stats.triangles += 1;
                    if reject_cutouts && t.c[3] > 0.5 {
                        continue;
                    }
                    let a = Vec3::from_slice(&t.a[..3]);
                    let e1 = Vec3::from_slice(&t.b[..3]) - a;
                    let e2 = Vec3::from_slice(&t.c[..3]) - a;
                    let p = direction.cross(e2);
                    let determinant = e1.dot(p);
                    if determinant.abs() < 0.0000001 {
                        continue;
                    }
                    let inverse_det = 1. / determinant;
                    let offset = origin - a;
                    let u = offset.dot(p) * inverse_det;
                    if !(0. ..=1.).contains(&u) {
                        continue;
                    }
                    let q = offset.cross(e1);
                    let v = direction.dot(q) * inverse_det;
                    if v < 0. || u + v > 1. {
                        continue;
                    }
                    let d = e2.dot(q) * inverse_det;
                    if d > 0.003 && d < distance {
                        distance = d;
                    }
                }
            }
        }
        current = if pending_len > 0 {
            pending_len -= 1;
            Some(pending[pending_len])
        } else {
            None
        };
    }
    (distance, stats)
}
pub(super) fn profile(name: &str, scene: &Scene, rays: &[(Vec3, Vec3)], reject: bool) {
    let groups = [
        ("camera", rays[..960].to_vec()),
        ("diffuse", rays[960..].iter().step_by(2).copied().collect()),
        ("sun", rays[961..].iter().step_by(2).copied().collect()),
    ];
    for (kind, rays) in groups {
        let mut baseline = Work::default();
        let mut expected = Vec::with_capacity(rays.len());
        let start = Instant::now();
        for &(o, d) in &rays {
            let (hit, work) = cast(scene, o, d, reject);
            expected.push(hit);
            baseline.nodes += work.nodes;
            baseline.candidates += work.candidates;
        }
        let baseline_cpu = start.elapsed();
        for capacity in [8, 16, 24] {
            let start = Instant::now();
            let mut candidate = Statistics::default();
            let mut overflow = 0;
            for (&(o, d), expected) in rays.iter().zip(&expected) {
                let (actual, work) = nearest(scene, o, d, reject, capacity);
                assert!(
                    (actual - expected).abs() < 0.0001,
                    "near-first hit differs: {actual} vs {expected}"
                );
                candidate.boxes += work.boxes;
                candidate.triangles += work.triangles;
                candidate.peak = candidate.peak.max(work.peak);
                overflow += usize::from(work.overflow);
            }
            println!(
                "{name} {kind} reject={reject} stack={capacity} rays={} boxes/ray {:.1}→{:.1} ({:.1}%) triangles/ray {:.1}→{:.1} ({:.1}%) peak={} overflow={} CPU {:?}→{:?}",
                rays.len(),
                baseline.nodes as f64 / rays.len() as f64,
                candidate.boxes as f64 / rays.len() as f64,
                candidate.boxes as f64 / baseline.nodes as f64 * 100.,
                baseline.candidates as f64 / rays.len() as f64,
                candidate.triangles as f64 / rays.len() as f64,
                candidate.triangles as f64 / baseline.candidates as f64 * 100.,
                candidate.peak,
                overflow,
                baseline_cpu,
                start.elapsed()
            );
        }
        // Deliberately force fallback and compare exact nearest distance.
        for &(o, d) in rays.iter().step_by(31) {
            assert!(
                (nearest(scene, o, d, reject, 0).0 - cast(scene, o, d, reject).0).abs() < 0.0001
            );
        }
    }
}

use super::*;
use std::{sync::Arc, time::Instant};

fn build_median(triangles: Vec<Triangle>) -> Scene {
    fn partition(scene: &mut Scene, first: usize, count: usize) {
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for triangle in &scene.triangles[first..first + count] {
            for p in [triangle.a, triangle.b, triangle.c] {
                let p = Vec3::from_slice(&p[..3]);
                low = low.min(p);
                high = high.max(p);
            }
        }
        let index = scene.nodes.len();
        scene.nodes.push(Node {
            min: (low - Vec3::splat(0.12)).to_array(),
            max: (high + Vec3::splat(0.12)).to_array(),
            first: first as u32,
            count: count as u32,
            escape: 0,
            padding: [0; 3],
        });
        if count > 8 {
            let extent = high - low;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let split = count / 2;
            scene.triangles[first..first + count].select_nth_unstable_by(split, |a, b| {
                let centroid = |t: &Triangle| t.a[axis] + t.b[axis] + t.c[axis];
                centroid(a).total_cmp(&centroid(b))
            });
            scene.nodes[index].count = 0;
            partition(scene, first, split);
            partition(scene, first + split, count - split);
        }
        scene.nodes[index].escape = scene.nodes.len() as u32;
    }
    let mut scene = Scene {
        water_offset: 0,

        triangles,
        nodes: Vec::new(),
        coverage: vec![0; 12],
    };
    let count = scene.triangles.len();
    if count > 0 {
        partition(&mut scene, 0, count);
    }
    scene
}
#[derive(Default, Debug)]
struct Work {
    nodes: usize,
    candidates: usize,
}
fn cast(scene: &Scene, origin: Vec3, direction: Vec3, reject_cutouts: bool) -> (f32, Work) {
    let mut distance = 512.0f32;
    let inverse = Vec3::from_array(
        direction
            .to_array()
            .map(|v| 1.0 / if v.abs() < 0.0000001 { 0.0000001 } else { v }),
    );
    let mut index = 0;
    let mut work = Work::default();
    while index < scene.nodes.len() {
        work.nodes += 1;
        let node = &scene.nodes[index];
        let a = (Vec3::from(node.min) - origin) * inverse;
        let b = (Vec3::from(node.max) - origin) * inverse;
        if a.min(b).max_element().max(0.0001) > a.max(b).min_element().min(distance) {
            index = node.escape as usize;
            continue;
        }
        if node.count == 0 {
            index += 1;
            continue;
        }
        for t in &scene.triangles[node.first as usize..(node.first + node.count) as usize] {
            work.candidates += 1;
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
            let inverse_det = 1.0 / determinant;
            let offset = origin - a;
            let u = offset.dot(p) * inverse_det;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = offset.cross(e1);
            let v = direction.dot(q) * inverse_det;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let d = e2.dot(q) * inverse_det;
            if d > 0.003 && d < distance {
                distance = d;
            }
        }
        index = node.escape as usize;
    }
    (distance, work)
}

#[test]
fn binned_split_preserves_nearest_hits_and_terminates_with_coincident_centroids() {
    let catalog = crate::content::catalog();
    let block = catalog.state_by_key("bloxgloom:cherry_leaves").unwrap();
    let mut chunk = crate::world::Chunk::from_blocks(
        crate::world::ChunkKey { x: 0, y: 0, z: 0 },
        0,
        vec![crate::world::AIR; crate::world::CHUNK_SIZE.pow(3)],
    );
    for i in 0..12 {
        chunk
            .blocks
            .set(crate::world::Chunk::index([i, 3, i]).unwrap(), block);
        chunk.blocks.set(
            crate::world::Chunk::index([i, 2, i]).unwrap(),
            crate::world::STONE,
        );
    }
    let mesh = crate::render::mesh::mesh_chunk(&chunk);
    let median = build_median(mesh.trace.triangles.clone());
    let sah = Scene::build([mesh.trace.clone()]);
    for x in 0..32 {
        for z in 0..32 {
            let origin = Vec3::new(x as f32 * 0.4, 10.0, z as f32 * 0.4);
            for reject in [false, true] {
                let a = cast(&median, origin, -Vec3::Y, reject).0;
                let b = cast(&sah, origin, -Vec3::Y, reject).0;
                assert!((a - b).abs() < 0.0001);
            }
        }
    }
    let coincident = Scene::build([Arc::new(Chunk {
        water: None,
        coarse_water: None,

        key: None,
        triangles: vec![mesh.trace.triangles[0]; 127],
    })]);
    assert_eq!(coincident.nodes[0].escape as usize, coincident.nodes.len());
    assert!(
        coincident
            .nodes
            .iter()
            .filter(|n| n.count > 0)
            .all(|n| n.count <= 8)
    );
    assert_eq!(coincident.nodes.iter().map(|n| n.count).sum::<u32>(), 127);
}

#[test]
#[ignore = "CPU-only real generated scene BVH traversal profile"]
fn generated_bvh_traversal_profile() {
    use crate::world::{ChunkKey, generate_chunk};
    const SEED: u64 = 0xB10C6100;
    let surface_height = |x: i32, z: i32| {
        for cy in (0..=crate::world::MAX_GENERATED_HEIGHT.div_euclid(16)).rev() {
            let chunk = generate_chunk(
                ChunkKey {
                    x: x.div_euclid(16),
                    y: cy,
                    z: z.div_euclid(16),
                },
                SEED,
            );
            for y in (0..16).rev() {
                if chunk
                    .block([x.rem_euclid(16) as usize, y, z.rem_euclid(16) as usize])
                    .unwrap()
                    != crate::world::AIR
                {
                    return cy * 16 + y as i32;
                }
            }
        }
        0
    };
    for (name, center, radius) in [("grove27", (-45, -128), 1), ("perf507", (0, 0), 6)] {
        let mut triangles = Vec::new();
        let height = surface_height(center.0 * 16, center.1 * 16);
        let cy = (height + 1).div_euclid(16);
        for z in center.1 - radius..=center.1 + radius {
            for x in center.0 - radius..=center.0 + radius {
                for y in cy - 1..=cy + 1 {
                    let chunk = generate_chunk(ChunkKey { x, y, z }, SEED);
                    triangles.extend_from_slice(
                        &crate::render::mesh::mesh_chunk(&chunk).trace.triangles,
                    );
                }
            }
        }
        let mut rays = Vec::new();
        let target = Vec3::new(
            (center.0 * 16) as f32,
            height as f32 + 3.0,
            (center.1 * 16) as f32,
        );
        let (target, eye) = if name == "perf507" {
            (
                Vec3::new(8.5, surface_height(8, 8) as f32 + 0.5, 8.5),
                Vec3::new(0.5, height as f32 + 18.0, -24.0),
            )
        } else {
            (target, target + Vec3::new(28.0, 13.0, 34.0))
        };

        let forward = (target - eye).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward);
        for y in 0..24 {
            for x in 0..40 {
                let d = (forward
                    + right * ((x as f32 + 0.5) / 40.0 - 0.5) * 1.3
                    + up * ((y as f32 + 0.5) / 24.0 - 0.5) * 0.8)
                    .normalize();
                rays.push((eye, d));
            }
        }
        for i in 0..1024 {
            let t = &triangles[i * triangles.len() / 1024];
            let p = (Vec3::from_slice(&t.a[..3])
                + Vec3::from_slice(&t.b[..3])
                + Vec3::from_slice(&t.c[..3]))
                / 3.0;
            let n = Vec3::from_slice(&t.normal[..3]);
            let theta = i as f32 * 2.3999631;
            let d = Vec3::new(theta.cos(), 0.2 + (i % 29) as f32 / 29.0, theta.sin()).normalize();
            rays.push((p + n * 0.01, d));
            rays.push((p + n * 0.01, Vec3::new(0.35, 0.85, 0.40).normalize()));
        }
        let start = Instant::now();
        let median = build_median(triangles.clone());
        let median_build = start.elapsed();
        let start = Instant::now();
        let sah = Scene::build([Arc::new(Chunk {
            water: None,
            coarse_water: None,

            triangles,
            key: None,
        })]);
        let sah_build = start.elapsed();
        let tight = Scene::build_with_bounds(
            [Arc::new(Chunk {
                water: None,
                coarse_water: None,

                triangles: median.triangles.clone(),
                key: None,
            })],
            true,
        );
        for reject in [false, true] {
            let mut a = Work::default();
            let mut b = Work::default();
            for &(o, d) in &rays {
                let (expected, w) = cast(&sah, o, d, reject);
                let (actual, v) = cast(&tight, o, d, reject);
                assert!(
                    (actual - expected).abs() < 0.0001,
                    "tight bounds nearest mismatch {actual} vs {expected}"
                );
                a.nodes += w.nodes;
                a.candidates += w.candidates;
                b.nodes += v.nodes;
                b.candidates += v.candidates;
            }
            println!(
                "{name} static bounds reject={reject}: nodes={}→{} boxes/ray={:.1}→{:.1} triangles/ray={:.1}→{:.1}",
                sah.nodes.len(),
                tight.nodes.len(),
                a.nodes as f64 / rays.len() as f64,
                b.nodes as f64 / rays.len() as f64,
                a.candidates as f64 / rays.len() as f64,
                b.candidates as f64 / rays.len() as f64
            );
        }
        println!(
            "{name}: triangles={}, nodes median={} sah={}, build median={median_build:?} sah={sah_build:?}",
            median.triangles.len(),
            median.nodes.len(),
            sah.nodes.len()
        );
        for reject in [false, true] {
            traversal::profile(name, &sah, &rays, reject);
            for (label, scene) in [("median", &median), ("sah16", &sah)] {
                let start = Instant::now();
                let mut work = Work::default();
                for &(origin, direction) in &rays {
                    let result = cast(scene, origin, direction, reject);
                    work.nodes += result.1.nodes;
                    work.candidates += result.1.candidates;
                }
                println!(
                    "{name} {label} cutouts_rejected={reject}: rays={} nodes/ray={:.1} candidates/ray={:.1} cpu={:?}",
                    rays.len(),
                    work.nodes as f64 / rays.len() as f64,
                    work.candidates as f64 / rays.len() as f64,
                    start.elapsed()
                );
            }
            for &(origin, direction) in &rays {
                assert!(
                    (cast(&median, origin, direction, reject).0
                        - cast(&sah, origin, direction, reject).0)
                        .abs()
                        < 0.001
                );
            }
        }
    }
}

#[path = "tests/traversal.rs"]
mod traversal;

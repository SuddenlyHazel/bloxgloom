//! Sampled regression only: no continuous-motion or additional head-look claim.
use super::*;

#[derive(Clone, Copy)]
struct Box3 {
    center: Vec3,
    axes: [Vec3; 3],
    half: [f32; 3],
    joint: usize,
    material: u32,
}

impl Box3 {
    fn from_vertices(vertices: &[CharacterVertex]) -> Self {
        assert_eq!(
            vertices.len(),
            36,
            "expected twelve deindexed cuboid triangles"
        );
        let joint = vertices[0].joint;
        let material = vertices[0].material;
        assert!(
            vertices
                .iter()
                .all(|v| v.joint == joint && v.material == material)
        );
        let mut basis = Vec::<Vec3>::new();
        for v in vertices {
            let normal = Vec3::from_array(v.normal);
            assert!((normal.length() - 1.0).abs() < 0.0001);
            if basis.iter().all(|axis| axis.dot(normal).abs() < 0.9999) {
                assert!(basis.iter().all(|axis| axis.dot(normal).abs() < 0.0001));
                basis.push(normal);
            }
        }
        assert_eq!(
            basis.len(),
            3,
            "cuboid requires three orthogonal normal axes"
        );
        let axes = [basis[0], basis[1], basis[2]];
        let min: [f32; 3] = std::array::from_fn(|a| {
            vertices
                .iter()
                .map(|v| axes[a].dot(Vec3::from_array(v.position)))
                .fold(f32::INFINITY, f32::min)
        });
        let max: [f32; 3] = std::array::from_fn(|a| {
            vertices
                .iter()
                .map(|v| axes[a].dot(Vec3::from_array(v.position)))
                .fold(f32::NEG_INFINITY, f32::max)
        });
        let half = std::array::from_fn(|a| (max[a] - min[a]) * 0.5);
        assert!(half.iter().all(|&extent| extent > 0.00001));
        let center = (0..3).map(|a| axes[a] * ((min[a] + max[a]) * 0.5)).sum();
        let mut corners = [false; 8];
        let mut faces = [0; 6];
        for triangle in vertices.chunks_exact(3) {
            let normal = Vec3::from_array(triangle[0].normal);
            let axis = axes
                .iter()
                .position(|axis| axis.dot(normal).abs() > 0.9999)
                .unwrap();
            let positive = axes[axis].dot(normal) > 0.0;
            faces[axis * 2 + usize::from(positive)] += 1;
            for v in triangle {
                assert!(Vec3::from_array(v.normal).abs_diff_eq(normal, 0.0001));
                let p = Vec3::from_array(v.position);
                let mut corner = 0;
                for a in 0..3 {
                    let value = axes[a].dot(p);
                    let low = (value - min[a]).abs() < 0.00001;
                    let high = (value - max[a]).abs() < 0.00001;
                    assert!(low || high, "vertex is not an OBB corner");
                    if high {
                        corner |= 1 << a;
                    }
                }
                corners[corner] = true;
                let plane = if positive { max[axis] } else { min[axis] };
                assert!(
                    (axes[axis].dot(p) - plane).abs() < 0.00001,
                    "normal must match its box face"
                );
            }
        }
        assert!(corners.into_iter().all(|present| present));
        assert_eq!(
            faces, [2; 6],
            "each cuboid face needs exactly two triangles"
        );
        Self {
            center,
            axes,
            half,
            joint,
            material,
        }
    }

    fn transformed(self, pose: &[Mat4; JOINT_COUNT]) -> Self {
        let matrix = pose[self.joint];
        let directions = self.axes.map(|axis| matrix.transform_vector3(axis));
        let axes = directions.map(Vec3::normalize);
        let half = std::array::from_fn(|a| self.half[a] * directions[a].length());
        let center = matrix.transform_point3(self.center);
        assert!(center.is_finite());
        Self {
            center,
            axes,
            half,
            ..self
        }
    }
}

// Positive is a conservative separating-axis distance, negative is penetration.
fn separating_margin(a: Box3, b: Box3) -> f32 {
    let mut margin = f32::NEG_INFINITY;
    let delta = b.center - a.center;
    let mut test = |axis: Vec3| {
        if axis.length_squared() < 1e-12 {
            return;
        }
        let axis = axis.normalize();
        let radius = |obb: Box3| {
            (0..3)
                .map(|i| obb.half[i] * axis.dot(obb.axes[i]).abs())
                .sum::<f32>()
        };
        margin = margin.max(delta.dot(axis).abs() - radius(a) - radius(b));
    };
    for axis in a.axes.into_iter().chain(b.axes) {
        test(axis);
    }
    for x in a.axes {
        for y in b.axes {
            test(x.cross(y));
        }
    }
    margin
}

// IDs are the append-only appearance/material identities, including every
// authored style. Exact cuboid counts make an accidentally omitted attachment
// fail before any clearance claim is made.
const HAIR_CUBOID_COUNTS: [usize; 13] = [10, 14, 22, 74, 49, 10, 12, 77, 55, 67, 118, 88, 28];

struct ClearanceFixture {
    body: Vec<Box3>,
    hair: Vec<Box3>,
}

impl ClearanceFixture {
    fn new(asset: &CharacterAsset) -> Self {
        assert_eq!(HAIR_PNGS.len(), HAIR_CUBOID_COUNTS.len());
        assert_eq!(asset.vertices.len() % 36, 0);
        assert_eq!(asset.indices.len(), asset.vertices.len());
        // This reconstruction is only valid while each deindexed block is one
        // complete cuboid. Fail explicitly if the asset layout changes.
        assert!(
            asset
                .indices
                .iter()
                .enumerate()
                .all(|(i, &index)| index as usize == i)
        );
        let boxes: Vec<_> = asset
            .vertices
            .chunks_exact(36)
            .map(Box3::from_vertices)
            .collect();
        let body: Vec<_> = boxes
            .iter()
            .copied()
            .filter(|b| b.material == 0 && b.joint != 1)
            .collect();
        let hair: Vec<_> = boxes.iter().copied().filter(|b| b.material > 0).collect();
        assert_eq!(body.len(), 5); // torso and limbs; scalp overlap excluded
        assert_eq!(hair.len(), HAIR_CUBOID_COUNTS.iter().sum::<usize>());
        for (index, &count) in HAIR_CUBOID_COUNTS.iter().enumerate() {
            assert_eq!(
                hair.iter()
                    .filter(|b| b.material == index as u32 + 1)
                    .count(),
                count,
                "missing or changed cuboids for hair ID {}",
                index + 1
            );
        }
        Self { body, hair }
    }

    fn assert_clear(&self, pose: &[Mat4; JOINT_COUNT], minima: &mut [f32; 13], context: &str) {
        let moved_body: Vec<_> = self.body.iter().map(|b| b.transformed(pose)).collect();
        for (cuboid, local) in self.hair.iter().enumerate() {
            let moved = local.transformed(pose);
            for target in &moved_body {
                let margin = separating_margin(moved, *target);
                minima[local.material as usize - 1] =
                    minima[local.material as usize - 1].min(margin);
                assert!(
                    margin >= -0.00001,
                    "sampled hair intersection: material {}, cuboid {}, body joint {}, {}, native SAT margin {} m",
                    local.material,
                    cuboid,
                    target.joint,
                    context,
                    margin
                );
            }
        }
    }
}

#[test]
fn sampled_original_clips_keep_all_hair_clear_of_torso_and_limbs() {
    let asset = CharacterAsset::builtin();
    let fixture = ClearanceFixture::new(&asset);
    let mut names: Vec<_> = asset.clips.iter().map(|clip| clip.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["crouch", "idle", "tool_use_left", "tool_use_right", "walk"]
    );
    for clip in &asset.clips {
        // 121 uniform times, including both endpoints, plus every authored key.
        // The imported source report is denser (1 ms); this smaller regression
        // exercises the production sampler without claiming continuous coverage.
        let mut times: Vec<f32> = (0..=120)
            .map(|step| clip.duration * step as f32 / 120.0)
            .collect();
        times.extend(
            clip.channels
                .iter()
                .flat_map(|channel| channel.times.iter().copied()),
        );
        times.sort_by(f32::total_cmp);
        times.dedup();
        let mut minima = [f32::INFINITY; 13];
        for &time in &times {
            fixture.assert_clear(
                &asset.sample(&clip.name, time),
                &mut minima,
                &format!("clip {}, time {} s", clip.name, time),
            );
        }
        eprintln!(
            "Sampled {} original {} poses; minimum native SAT margins by hair IDs 1..13: {minima:?} m. No added head-look; not continuous proof.",
            times.len(),
            clip.name
        );
    }
}

#[test]
fn sampled_idle_walk_blends_keep_all_hair_clear_of_torso_and_limbs() {
    let asset = CharacterAsset::builtin();
    let fixture = ClearanceFixture::new(&asset);
    let duration = |name| {
        asset
            .clips
            .iter()
            .find(|clip| clip.name == name)
            .unwrap()
            .duration
    };
    let mut minima = [f32::INFINITY; 13];
    // 9×17 distinct phases over [0, period), including zero; nine blend weights
    // include both endpoints. Looping clip endpoints duplicate phase zero.
    for idle in 0..9 {
        for walk in 0..17 {
            for weight in 0..9 {
                fixture.assert_clear(
                    &asset.sample_blended(
                        duration("idle") * idle as f32 / 9.0,
                        duration("walk") * walk as f32 / 17.0,
                        weight as f32 / 8.0,
                    ),
                    &mut minima,
                    &format!("idle phase {idle}/9, walk phase {walk}/17, weight {weight}/8"),
                );
            }
        }
    }
    eprintln!(
        "Sampled 1377 idle/walk blends; minimum native SAT margins by hair IDs 1..13: {minima:?} m. Not continuous or extra-head-look proof."
    );
}

#[test]
fn sat_detects_overlap_contact_separation_and_rotated_cross_axes() {
    let unit = Box3 {
        center: Vec3::ZERO,
        axes: [Vec3::X, Vec3::Y, Vec3::Z],
        half: [0.5; 3],
        joint: 0,
        material: 0,
    };
    assert!(separating_margin(unit, unit) < -0.99);
    assert!(
        separating_margin(
            unit,
            Box3 {
                center: Vec3::X,
                ..unit
            }
        )
        .abs()
            < 0.00001
    );
    assert!(
        (separating_margin(
            unit,
            Box3 {
                center: Vec3::X * 1.2,
                ..unit
            }
        ) - 0.2)
            .abs()
            < 0.00001
    );
    let rotation = Quat::from_rotation_y(0.63) * Quat::from_rotation_x(0.41);
    let rotated = Box3 {
        axes: unit.axes.map(|a| rotation * a),
        ..unit
    };
    assert!(separating_margin(unit, rotated) < 0.0);
    assert!(
        separating_margin(
            unit,
            Box3 {
                center: Vec3::new(2.0, 1.7, 2.0),
                ..rotated
            }
        ) > 0.0
    );
}

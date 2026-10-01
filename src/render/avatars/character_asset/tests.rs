use super::*;

#[test]
fn both_native_bodies_share_thirty_joints_and_real_scale() {
    let asset = CharacterAsset::builtin();
    assert_eq!(asset.vertices.len(), 28168);
    assert_eq!(asset.indices.len(), 40944);
    assert_eq!(asset.joints.len(), 30);
    let rest = asset.sample("rest", 0.0);
    for material in [0, 14] {
        let body: Vec<_> = asset
            .vertices
            .iter()
            .filter(|v| v.material == material)
            .map(|v| rest[v.joint].transform_point3(Vec3::from_array(v.position)))
            .collect();
        let min = body.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max = body.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        assert!(
            min.abs() < 0.00001 && (max - 1.8).abs() < 0.00001,
            "body {material}: {min}..{max}"
        );
    }
    assert_eq!(asset.joints[5].name, "head");
    assert_eq!(asset.joints[10].name, "grip_R");
    assert_eq!(asset.joints[22].name, "grip_L");
    assert!(
        asset
            .vertices
            .iter()
            .filter(|v| (1..14).contains(&v.material))
            .all(|v| v.joint == 5)
    );
}

#[test]
fn procedural_motion_loops_and_oneshots_clamp_without_old_rig_tracks() {
    let asset = CharacterAsset::builtin();
    assert_eq!(asset.clips.len(), 4);
    assert!(
        asset
            .clips
            .iter()
            .all(|c| !["idle", "walk", "run"].contains(&c.name.as_str()))
    );
    for (clip, duration) in [("idle", 3.0), ("walk", 0.8), ("run", 0.8)] {
        for (a, b) in asset
            .sample(clip, 0.2)
            .iter()
            .zip(asset.sample(clip, 0.2 + duration))
        {
            assert!(a.abs_diff_eq(b, 0.00001));
        }
    }
    assert_eq!(asset.sample("crouch", 0.6), asset.sample("crouch", 600.0));
    for clip in ["tool_use_left", "tool_use_right"] {
        for (a, b) in asset
            .sample(clip, 0.8)
            .iter()
            .zip(asset.sample("rest", 0.0))
        {
            assert!(a.abs_diff_eq(b, 0.00001));
        }
        assert_eq!(asset.sample(clip, 0.8), asset.sample(clip, 600.0));
    }
}

#[test]
fn both_body_basis_and_normals_keep_forward_winding() {
    let asset = CharacterAsset::builtin();
    let rest = asset.sample("rest", 0.0);
    for material in [0, 14] {
        for v in asset
            .vertices
            .iter()
            .filter(|v| v.material == material && v.joint == 5 && v.normal[2] < -0.9)
        {
            let point = rest[5].transform_point3(Vec3::from_array(v.position));
            let normal = rest[5].transform_vector3(Vec3::from_array(v.normal));
            assert!(point.z > 0.0 && normal.z > 0.9);
        }
    }
}

#[test]
fn every_hair_style_follows_the_actual_head_in_source_and_gameplay_motion() {
    let asset = CharacterAsset::builtin();
    let rest = asset.sample("rest", 0.0);
    let names: Vec<_> = asset
        .clips
        .iter()
        .map(|c| c.name.as_str())
        .chain([
            "idle",
            "walk",
            "run",
            "crouch",
            "tool_use_left",
            "tool_use_right",
        ])
        .collect();
    for clip in names {
        for step in 0..=60 {
            let pose = asset.sample(clip, step as f32 / 60.0);
            let delta = pose[5] * rest[5].inverse();
            assert!(
                pose.iter()
                    .all(|m| m.is_finite() && (m.determinant() - 1.0).abs() < 0.0001)
            );
            for v in asset
                .vertices
                .iter()
                .filter(|v| (1..14).contains(&v.material))
            {
                let point = Vec3::from_array(v.position);
                assert!(pose[5].transform_point3(point).abs_diff_eq(
                    delta.transform_point3(rest[5].transform_point3(point)),
                    0.00001
                ));
            }
        }
    }
}

#[test]
fn native_materials_match_manifest_and_fixed_accessories_are_not_hair_tint() {
    let asset = CharacterAsset::builtin();
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../assets/models/player/articulated/manifest.json"
    ))
    .unwrap();
    for (i, record) in manifest["records"].as_array().unwrap().iter().enumerate() {
        assert_eq!(record["id"], i);
        assert_eq!(
            record["vertices"].as_u64().unwrap() as usize,
            asset
                .vertices
                .iter()
                .filter(|v| v.material == i as u32)
                .count()
        );
        if (1..14).contains(&i) {
            assert_eq!(record["key"], crate::appearance::HAIR[i]);
        }
    }
    assert!(
        asset
            .vertices
            .iter()
            .any(|v| v.material == 12 && v.surface == 8)
    );
    assert!(
        asset
            .vertices
            .iter()
            .any(|v| v.material == 12 && v.surface == 7)
    );
    assert_eq!(HAIR_PNGS.len(), 26);
    for bytes in HAIR_PNGS
        .into_iter()
        .chain(EYE_PNGS)
        .chain(MOUTH_PNGS)
        .chain(IRIS_MASK_PNGS)
    {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (32, 32));
    }
}

#[test]
fn corrupt_assets_fail_before_gpu_allocation() {
    for mutation in 0..6 {
        let mut asset = CharacterAsset::builtin();
        match mutation {
            0 => asset.vertices[0].joint = JOINT_COUNT,
            1 => asset.joints[1].parent = Some(1),
            2 => asset.clips[0].channels[0].times[1] = -1.0,
            3 => asset.vertices[0].surface = 99,
            4 => asset.vertices[0].material = 99,
            _ => {
                asset
                    .vertices
                    .iter_mut()
                    .find(|v| v.material == 1)
                    .unwrap()
                    .joint = 0
            }
        }
        assert!(asset.validate().is_err());
    }
    let mut disjoint = CharacterAsset::builtin();
    disjoint.indices.extend(disjoint.indices[..3].to_vec());
    assert!(disjoint.validate().is_err());
    assert!(CharacterAsset::parse("{}").is_err());
}

#[test]
fn local_blending_is_finite_and_preserves_endpoint_poses() {
    let asset = CharacterAsset::builtin();
    for (weight, clip, time) in [(0.0, "idle", 0.4), (1.0, "walk", 0.2)] {
        for (a, b) in asset
            .sample_blended(0.4, 0.2, weight)
            .iter()
            .zip(asset.sample(clip, time))
        {
            assert!(a.abs_diff_eq(b, 0.00001));
        }
    }
    for step in 0..=100 {
        assert!(
            asset
                .sample_blended(step as f32 / 30.0, step as f32 / 40.0, step as f32 / 100.0)
                .iter()
                .all(|m| m.is_finite() && m.determinant() > 0.0)
        );
    }
}

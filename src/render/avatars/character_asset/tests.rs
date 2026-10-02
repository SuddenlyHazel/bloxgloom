use super::*;

#[test]
fn native_master_has_all_authored_clips_embedded_textures_and_supported_parts() {
    let asset = CharacterAsset::builtin();
    assert_eq!(asset.joints.len(), JOINT_COUNT);
    assert_eq!(asset.images.len(), 17);
    assert_eq!(asset.animation.clips.len(), 11);
    for (name, duration) in [
        ("idle", 3.0),
        ("walk", 1.2),
        ("run", 0.72),
        ("crouch", 0.3),
        ("tool_use_left", 0.8),
        ("tool_use_right", 0.8),
    ] {
        assert!((asset.duration(name) - duration).abs() < 1e-5);
    }
    for group in 0..MATERIAL_COUNT {
        assert!(asset.vertices.iter().any(|v| v.material == group as u32));
    }
    assert!(
        asset
            .vertices
            .iter()
            .filter(|v| (1..14).contains(&v.material))
            .all(|v| v.joint == rig::HEAD)
    );
    assert!(
        asset
            .vertices
            .iter()
            .any(|v| v.surface == 8 && v.material == 12)
    );
    assert!(
        asset
            .vertices
            .iter()
            .any(|v| v.surface == 7 && v.material == 12)
    );
    let pose = asset.sample("rest", 0.0);
    for group in [0, 14] {
        let points: Vec<_> = asset
            .vertices
            .iter()
            .filter(|v| v.material == group)
            .map(|v| pose[v.joint].transform_point3(Vec3::from_array(v.position)))
            .collect();
        let min = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        assert!(min.abs() < 1e-5 && (max - 1.8).abs() < 1e-5);
    }
}

#[test]
fn compiled_live_geometry_matches_native_glb_tracks_without_procedural_overrides() {
    let asset = CharacterAsset::builtin();
    let controls = serde_json::from_slice(include_bytes!(
        "../../../../assets/models/player/master/controls.json"
    ))
    .unwrap();
    let source = Model::from_glb(
        include_bytes!("../../../../assets/models/player/master/model.glb"),
        controls,
    )
    .unwrap();
    let basis = Mat4::from_rotation_y(std::f32::consts::PI);
    for clip in &source.clips {
        for t in [0.0, clip.duration * 0.25, clip.duration * 0.75] {
            let original = source.sample(Some(&clip.name), t).unwrap();
            let compiled = asset.sample(&clip.name, t);
            for group in [0, 1, 12, 14] {
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
                let look = serde_json::from_value(
                    serde_json::json!({"variants":{"body":body,"hair_style":hair}}),
                )
                .unwrap();
                let visible = source.appearance(&look).unwrap().visible;
                let expected: Vec<_> = source
                    .primitives
                    .iter()
                    .filter(|p| visible[p.node] && (p.material != 0) == (1..14).contains(&group))
                    .flat_map(|p| &p.vertices)
                    .map(|v| {
                        basis
                            * original[v.joints[0] as usize]
                            * glam::Vec4::from((Vec3::from_array(v.position), 1.0))
                    })
                    .collect();
                let actual: Vec<_> = asset
                    .vertices
                    .iter()
                    .filter(|v| v.material == group as u32)
                    .map(|v| compiled[v.joint].transform_point3(Vec3::from_array(v.position)))
                    .collect();
                assert_eq!(actual.len(), expected.len());
                for (a, b) in actual.iter().zip(expected) {
                    assert!(
                        a.abs_diff_eq(b.truncate(), 1e-5),
                        "{} at {t}, group {group}: {a:?} {b:?}",
                        clip.name
                    );
                }
            }
        }
    }
}

#[test]
fn baked_loops_wrap_and_tool_oneshots_clamp() {
    let asset = CharacterAsset::builtin();
    for clip in ["idle", "walk", "run"] {
        for (a, b) in asset
            .sample(clip, 0.2)
            .iter()
            .zip(asset.sample(clip, 0.2 + asset.duration(clip)))
        {
            assert!(a.abs_diff_eq(b, 1e-5));
        }
    }
    for clip in ["crouch", "tool_use_left", "tool_use_right"] {
        assert_eq!(
            asset.sample(clip, asset.duration(clip)),
            asset.sample(clip, 600.0)
        );
    }
}

#[test]
fn local_blending_keeps_authored_endpoints_and_finite_transforms() {
    let asset = CharacterAsset::builtin();
    for (weight, clip, time) in [(0.0, "idle", 0.4), (1.0, "walk", 0.2)] {
        for (a, b) in asset
            .sample_blended(0.4, 0.2, weight)
            .iter()
            .zip(asset.sample(clip, time))
        {
            assert!(a.abs_diff_eq(b, 1e-5));
        }
    }
}

use super::*;

#[test]
fn authored_geometry_has_correct_scale_and_socket() {
    let asset = CharacterAsset::builtin();
    assert_eq!(asset.vertices.len(), 1080);
    assert_eq!(asset.indices.len(), 1080);
    let matrices = asset.sample("rest", 0.0);
    let body: Vec<_> = asset
        .vertices
        .iter()
        .filter(|v| v.material == 0)
        .map(|v| matrices[v.joint].transform_point3(Vec3::from_array(v.position)))
        .collect();
    let min = body.iter().fold(f32::INFINITY, |min, v| min.min(v.y));
    let max = body.iter().fold(f32::NEG_INFINITY, |max, v| max.max(v.y));
    assert!(min.abs() < 0.00001);
    assert!((max - 1.8).abs() < 0.00001);
    assert!(
        asset
            .vertices
            .iter()
            .filter(|v| v.material > 0)
            .all(|v| v.joint == 1)
    );
    assert_eq!(asset.joints[1].name, "head");
}

#[test]
fn looping_and_held_clips_keep_authored_timing() {
    let asset = CharacterAsset::builtin();
    assert_eq!(asset.sample("crouch", 0.6), asset.sample("crouch", 600.0));
    for (a, b) in asset
        .sample("walk", 0.2)
        .iter()
        .zip(asset.sample("walk", 1.0))
    {
        assert!(a.abs_diff_eq(b, 0.00001));
    }
    assert_ne!(asset.sample("idle", 0.0), asset.sample("crouch", 0.6));
    assert_ne!(
        asset.sample("tool_use_left", 0.4),
        asset.sample("tool_use_right", 0.4)
    );
    for (weight, clip, time) in [(0.0, "idle", 0.4), (1.0, "walk", 0.2)] {
        for (a, b) in asset
            .sample_blended(0.4, 0.2, weight)
            .iter()
            .zip(asset.sample(clip, time))
        {
            assert!(a.abs_diff_eq(b, 0.00001));
        }
    }
}

#[test]
fn face_uv_and_forward_basis_preserve_upright_pixels() {
    let asset = CharacterAsset::builtin();
    let rest = asset.sample("rest", 0.0);
    let front: Vec<_> = asset
        .vertices
        .iter()
        .filter(|v| v.material == 0 && v.joint == 1 && v.normal == [0.0, 0.0, -1.0])
        .collect();
    assert_eq!(front.len(), 6);
    for vertex in front {
        let position = rest[1].transform_point3(Vec3::from_array(vertex.position));
        let normal = rest[1]
            .transform_vector3(Vec3::from_array(vertex.normal))
            .normalize();
        assert!(position.z > 0.0);
        assert!(normal.abs_diff_eq(Vec3::Z, 0.00001));
        assert!((32.0..=64.0).contains(&(vertex.uv[0] * 512.0)));
        assert!((32.0..=64.0).contains(&(vertex.uv[1] * 256.0)));
        // PNG top row maps to the top of the face without flipping V.
        if vertex.position[1] > 0.0 {
            assert_eq!(vertex.uv[1] * 256.0, 32.0);
        } else {
            assert_eq!(vertex.uv[1] * 256.0, 64.0);
        }
    }
}

#[test]
fn hair_stays_rigidly_attached_through_crouch_and_tool_rotation() {
    let asset = CharacterAsset::builtin();
    let rest = asset.sample("rest", 0.0);
    for (clip, time) in [
        ("crouch", 0.6),
        ("tool_use_left", 0.4),
        ("tool_use_right", 0.4),
    ] {
        let animated = asset.sample(clip, time);
        let head_delta = animated[1] * rest[1].inverse();
        for vertex in asset.vertices.iter().filter(|v| v.material > 0) {
            let local = Vec3::from_array(vertex.position);
            let expected = head_delta.transform_point3(rest[1].transform_point3(local));
            let actual = animated[vertex.joint].transform_point3(local);
            assert!(actual.abs_diff_eq(expected, 0.00001));
        }
    }
    let held = asset.sample("crouch", 0.6);
    assert!(!held[1].abs_diff_eq(rest[1], 0.001));
}

#[test]
fn malformed_assets_fail_before_rendering() {
    let mut asset = CharacterAsset::builtin();
    asset.vertices[0].joint = JOINT_COUNT;
    assert!(asset.validate().is_err());
    let mut asset = CharacterAsset::builtin();
    asset.joints[1].parent = Some(1);
    assert!(asset.validate().is_err());
    let mut asset = CharacterAsset::builtin();
    asset.clips[0].channels[0].times[1] = -1.0;
    assert!(asset.validate().is_err());
    assert!(CharacterAsset::parse("{}").is_err());
}

#[test]
fn both_tools_return_to_rest_and_idle_walk_transitions_stay_finite() {
    let asset = CharacterAsset::builtin();
    let rest = asset.sample("rest", 0.0);
    for name in ["tool_use_left", "tool_use_right"] {
        let clip = asset.clips.iter().find(|clip| clip.name == name).unwrap();
        let last = asset.sample(name, clip.duration);
        assert_eq!(last, asset.sample(name, 600.0), "one-shots must not loop");
        for (actual, expected) in last.iter().zip(rest) {
            assert!(
                actual.abs_diff_eq(expected, 0.00001),
                "tool should return to rest"
            );
        }
    }
    let idle = asset.clips.iter().find(|clip| clip.name == "idle").unwrap();
    for (a, b) in asset
        .sample("idle", 0.1)
        .iter()
        .zip(asset.sample("idle", idle.duration + 0.1))
    {
        assert!(a.abs_diff_eq(b, 0.00001));
    }
    for step in 0..=100 {
        let matrices =
            asset.sample_blended(step as f32 / 30.0, step as f32 / 40.0, step as f32 / 100.0);
        assert!(
            matrices
                .iter()
                .all(|matrix| matrix.is_finite() && matrix.determinant() > 0.0)
        );
    }
}

#[test]
fn authored_material_ids_and_face_layers_stay_bounded() {
    let asset = CharacterAsset::builtin();
    for (material, expected) in [(0, 216), (1, 360), (2, 504)] {
        assert_eq!(
            asset
                .vertices
                .iter()
                .filter(|v| v.material == material)
                .count(),
            expected
        );
    }
    let mut invalid = asset.clone();
    invalid.vertices[0].material = 3;
    assert!(invalid.validate().is_err());
    let mut detached = asset;
    let hair = detached
        .vertices
        .iter_mut()
        .find(|v| v.material == 2)
        .unwrap();
    hair.joint = 0;
    assert!(detached.validate().is_err());
    assert_eq!(EYE_PNGS.len(), EYE_NAMES.len());
    assert_eq!(EYE_PNGS.len(), IRIS_MASK_PNGS.len());
    assert_eq!(MOUTH_PNGS.len(), MOUTH_NAMES.len());
    assert_eq!(
        EYE_NAMES,
        [
            "classic",
            "cute_glint",
            "kawaii_star",
            "playful_wink",
            "happy_crescent",
            "neon_focus",
            "neon_curious",
            "soft_sleepy"
        ]
    );
    assert_eq!(
        MOUTH_NAMES,
        [
            "classic",
            "soft_smile",
            "cat_smile",
            "tiny_open",
            "playful",
            "smirk"
        ]
    );
    for bytes in std::iter::once(CLEAN_FACE_PNG)
        .chain(EYE_PNGS)
        .chain(MOUTH_PNGS)
        .chain(IRIS_MASK_PNGS)
    {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (32, 32));
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
        assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    }
    let decoder = png::Decoder::new(std::io::Cursor::new(HAIR_UNDERCUT_PNG));
    let reader = decoder.read_info().unwrap();
    assert_eq!((reader.info().width, reader.info().height), (32, 32));
}

#[test]
fn gameplay_tools_overlay_crouch_and_walk_without_replacing_the_legs() {
    let asset = CharacterAsset::builtin();
    let crouched_walk = asset.sample_gameplay(0.35, 0.2, 1.0, 1.0, None);
    let standing_walk = asset.sample_gameplay(0.35, 0.2, 1.0, 0.0, None);
    assert!(!crouched_walk[0].abs_diff_eq(standing_walk[0], 0.001));
    assert!(!crouched_walk[5].abs_diff_eq(standing_walk[5], 0.001));
    let still_crouch = asset.sample_gameplay(0.35, 0.2, 0.0, 1.0, None);
    assert!(!crouched_walk[5].abs_diff_eq(still_crouch[5], 0.001));
    for right in [false, true] {
        let active = asset.sample_gameplay(0.35, 0.2, 1.0, 1.0, Some((right, 0.4)));
        for index in [0, 5, 6] {
            assert!(active[index].abs_diff_eq(crouched_walk[index], 0.00001));
        }
        let arm = if right { 4 } else { 3 };
        assert!(!active[arm].abs_diff_eq(crouched_walk[arm], 0.001));
        assert!(!active[1].abs_diff_eq(crouched_walk[1], 0.001));
        let clip = asset
            .clips
            .iter()
            .find(|clip| {
                clip.name
                    == if right {
                        "tool_use_right"
                    } else {
                        "tool_use_left"
                    }
            })
            .unwrap();
        assert!((clip.duration - tool_duration(right)).abs() < 0.00001);
        let completed =
            asset.sample_gameplay(0.35, 0.2, 1.0, 1.0, Some((right, tool_duration(right))));
        let held = asset.sample_gameplay(0.35, 0.2, 1.0, 1.0, Some((right, 600.0)));
        for (index, matrix) in completed.iter().enumerate() {
            assert!(matrix.abs_diff_eq(crouched_walk[index], 0.00001));
            assert!(matrix.abs_diff_eq(held[index], 0.00001));
        }
    }
}

#[test]
fn gameplay_blends_remain_finite_and_no_layers_preserve_locomotion() {
    let asset = CharacterAsset::builtin();
    for step in 0..=100 {
        let weight = step as f32 / 100.0;
        let locomotion = asset.sample_blended(0.35, 0.2, weight);
        let no_layers = asset.sample_gameplay(0.35, 0.2, weight, 0.0, None);
        for (actual, expected) in no_layers.iter().zip(locomotion) {
            assert!(actual.abs_diff_eq(expected, 0.00001));
        }
        let layered = asset.sample_gameplay(0.35, 0.2, weight, weight, Some((true, weight * 0.8)));
        assert!(
            layered
                .iter()
                .all(|matrix| matrix.is_finite() && matrix.determinant() > 0.0)
        );
    }
    let invalid = asset.sample_gameplay(
        f32::NAN,
        f32::INFINITY,
        f32::NAN,
        f32::NAN,
        Some((false, f32::NAN)),
    );
    assert!(invalid.iter().all(|matrix| matrix.is_finite()));
}

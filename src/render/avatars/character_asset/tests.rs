use super::*;

#[test]
fn authored_geometry_has_correct_scale_and_socket() {
    let asset = CharacterAsset::builtin();
    assert_eq!(asset.vertices.len(), 576);
    assert_eq!(asset.indices.len(), 576);
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
            .filter(|v| v.material == 1)
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
        for vertex in asset.vertices.iter().filter(|v| v.material == 1) {
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

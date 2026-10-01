use super::*;

#[test]
fn looking_down_keeps_attached_arms_and_framing_preserves_body() {
    let mut pose = [Mat4::IDENTITY; 7];
    let original = [Mat4::from_translation(Vec3::new(0.0, 0.0, -0.18)); 7];
    View {
        id: 1,
        eye_height: 1.62,
        pitch: -1.2,
    }
    .prepare_pose(&mut pose);
    assert_eq!(pose, original);
    pose = [Mat4::IDENTITY; 7];
    View {
        id: 1,
        eye_height: 1.05,
        pitch: 0.2,
    }
    .prepare_pose(&mut pose);
    for i in [0, 1, 2, 5, 6] {
        assert_eq!(pose[i], original[i]);
    }
    for i in [3, 4] {
        assert!(pose[i].is_finite());
        assert_ne!(pose[i], original[i]);
        assert!(pose[i].w_axis.z > 0.0, "hands frame in front of the eye");
    }
}

#[test]
fn authored_tool_swing_stays_in_front_of_the_first_person_eye() {
    let asset = crate::render::avatars::character_asset::CharacterAsset::builtin();
    let view = View {
        id: 1,
        eye_height: 1.6,
        pitch: 0.0,
    };
    let mut idle = asset.sample_gameplay(0.0, 0.0, 0.0, 0.0, None);
    let mut swing = asset.sample_gameplay(0.0, 0.0, 0.0, 0.0, Some((true, 0.3)));
    view.prepare_pose(&mut idle);
    view.prepare_pose(&mut swing);
    let hand = Vec3::new(0.0, -0.65, 0.0);
    assert!(
        swing[4].transform_point3(hand).z > 0.0,
        "engine-space forward is +Z"
    );
    assert!(
        swing[4]
            .transform_point3(hand)
            .distance(idle[4].transform_point3(hand))
            > 0.2
    );
    for i in [0, 2, 5, 6] {
        assert_eq!(swing[i], idle[i]);
    }
}

#[test]
fn forward_view_keeps_resting_shoulders_below_the_viewport() {
    let asset = crate::render::avatars::character_asset::CharacterAsset::builtin();
    for eye_height in [1.6, 1.05] {
        let mut pose = asset.sample_gameplay(0.0, 0.0, 0.0, 0.0, None);
        View {
            id: 1,
            eye_height,
            pitch: 0.0,
        }
        .prepare_pose(&mut pose);
        // Include the widest supported FOV, so shoulder caps stay out of view
        // in ordinary first-person settings as well as the default 70 degrees.
        let bottom = -(110.0f32 / 2.0).to_radians().tan();
        for vertex in asset
            .vertices
            .iter()
            .filter(|v| matches!(v.joint, 3 | 4) && v.position[1] >= -0.01)
        {
            let point = pose[vertex.joint].transform_point3(Vec3::from_array(vertex.position))
                - Vec3::Y * eye_height;
            assert!(
                point.z <= 0.0 || point.y / point.z < bottom,
                "shoulder cap entered viewport: {point:?}"
            );
        }
    }
}

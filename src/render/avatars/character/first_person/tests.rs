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

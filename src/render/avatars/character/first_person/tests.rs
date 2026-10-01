use super::*;
use crate::render::avatars::character_asset::CharacterAsset;

#[test]
fn looking_down_keeps_attached_arms_and_framing_preserves_body() {
    let asset = CharacterAsset::builtin();
    let original = asset.sample_gameplay(0.3, 0.2, 1.0, 0.0, None);
    let offset = Mat4::from_translation(Vec3::new(0.0, 0.0, -0.35));
    let mut pose = original;
    View {
        id: 1,
        eye_height: 1.6,
        pitch: -1.2,
    }
    .prepare_pose(&mut pose, None);
    for i in 0..JOINT_COUNT {
        assert!(pose[i].abs_diff_eq(offset * original[i], 1e-6));
    }
    pose = original;
    View {
        id: 1,
        eye_height: 1.6,
        pitch: 0.2,
    }
    .prepare_pose(&mut pose, None);
    for i in 0..JOINT_COUNT {
        assert!(pose[i].is_finite());
        if !RIGHT_ARM.contains(&i) && !LEFT_ARM.contains(&i) {
            assert!(pose[i].abs_diff_eq(offset * original[i], 1e-6));
        }
    }
}

#[test]
fn every_arm_joint_and_grip_share_one_rigid_first_person_delta() {
    let asset = CharacterAsset::builtin();
    for time in [0.0, 0.12, 0.24, 0.4, 0.79] {
        for pitch in [-1.2, -0.7, 0.0, 1.2] {
            for right in [false, true] {
                let original = asset.sample_gameplay(0.3, 0.2, 0.6, 0.5, Some((right, time)));
                let mut framed = original;
                View {
                    id: 1,
                    eye_height: 1.3,
                    pitch,
                }
                .prepare_pose(&mut framed, Some((right, time)));
                for arm in [RIGHT_ARM, LEFT_ARM] {
                    for pair in arm.windows(2) {
                        let before = original[pair[0]].inverse() * original[pair[1]];
                        let after = framed[pair[0]].inverse() * framed[pair[1]];
                        assert!(
                            before.abs_diff_eq(after, 1e-5),
                            "disconnected arm at {pair:?}"
                        );
                    }
                }
                for hand in [false, true] {
                    assert!(grip_anchor(&framed, hand).is_finite());
                    assert!(grip_anchor(&framed, hand).determinant() > 0.99);
                }
            }
        }
    }
}

#[test]
fn either_tool_swing_brings_its_own_grip_forward() {
    let asset = CharacterAsset::builtin();
    let view = View {
        id: 1,
        eye_height: 1.6,
        pitch: 0.0,
    };
    for right in [false, true] {
        let mut idle = asset.sample_gameplay(0.0, 0.0, 0.0, 0.0, None);
        let mut swing = asset.sample_gameplay(0.0, 0.0, 0.0, 0.0, Some((right, 0.24)));
        view.prepare_pose(&mut idle, None);
        view.prepare_pose(&mut swing, Some((right, 0.24)));
        let hand = grip_anchor(&swing, right).transform_point3(Vec3::ZERO);
        let resting = grip_anchor(&idle, right).transform_point3(Vec3::ZERO);
        assert!(hand.z > 0.0, "engine-space forward is +Z: {hand:?}");
        assert!(hand.distance(resting) > 0.2);
    }
}

#[test]
fn forward_view_keeps_resting_shoulder_caps_below_the_viewport() {
    let asset = CharacterAsset::builtin();
    for eye_height in [1.6, 1.05] {
        let mut pose = asset.sample_gameplay(0.0, 0.0, 0.0, 0.0, None);
        View {
            id: 1,
            eye_height,
            pitch: 0.0,
        }
        .prepare_pose(&mut pose, None);
        let bottom = -(110.0f32 / 2.0).to_radians().tan();
        for vertex in asset.vertices.iter().filter(|v| {
            (v.joint == RIGHT_ARM[1] || v.joint == LEFT_ARM[1]) && v.position[1] >= -0.01
        }) {
            let point = pose[vertex.joint].transform_point3(Vec3::from_array(vertex.position))
                - Vec3::Y * eye_height;
            assert!(
                point.z <= 0.0 || point.y / point.z < bottom,
                "shoulder cap entered viewport: {point:?}"
            );
        }
    }
}

#[test]
fn active_hand_and_forearm_project_inside_the_view_at_mid_swing() {
    let asset = CharacterAsset::builtin();
    for right in [false, true] {
        let arm = if right { RIGHT_ARM } else { LEFT_ARM };
        for (crouch, eye_height) in [(0.0, 1.6), (1.0, 1.0575)] {
            for time in [0.3, 0.4] {
                for pitch in [-0.2, 0.0, 0.2] {
                    let mut pose =
                        asset.sample_gameplay(0.35, 0.2, crouch, crouch, Some((right, time)));
                    View {
                        id: 1,
                        eye_height,
                        pitch,
                    }
                    .prepare_pose(&mut pose, Some((right, time)));
                    let into_view = Quat::from_rotation_x(pitch);
                    for fov in [60.0_f32, 70.0, 110.0] {
                        let tangent = (fov.to_radians() * 0.5).tan();
                        for joint in [arm[2], arm[3]] {
                            let projected: Vec<_> = asset
                                .vertices
                                .iter()
                                .filter(|v| v.material == 0 && v.joint == joint)
                                .filter_map(|vertex| {
                                    let point = into_view
                                        * (pose[joint]
                                            .transform_point3(Vec3::from_array(vertex.position))
                                            - Vec3::Y * eye_height);
                                    (point.z > 0.05).then(|| {
                                        glam::Vec2::new(
                                            point.x / (point.z * tangent * (16.0 / 9.0)),
                                            point.y / (point.z * tangent),
                                        )
                                    })
                                })
                                .collect();
                            let visible = projected
                                .iter()
                                .filter(|point| point.x.abs() < 1.0 && point.y.abs() < 1.0)
                                .count();
                            assert!(
                                visible >= 4,
                                "invisible joint {joint}: right={right}, crouch={crouch}, t={time}, pitch={pitch}, fov={fov}, visible={visible}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn tool_framing_returns_to_rest_and_crouch_chest_leaves_the_crosshair_clear() {
    let asset = CharacterAsset::builtin();
    let view = View {
        id: 1,
        eye_height: 1.6,
        pitch: 0.0,
    };
    let base = asset.sample_gameplay(0.35, 0.0, 0.0, 0.0, None);
    let mut rest = base;
    view.prepare_pose(&mut rest, None);
    for right in [false, true] {
        for time in [0.0, 0.8, 20.0, f32::NAN] {
            let mut pose = base;
            view.prepare_pose(&mut pose, Some((right, time)));
            for (actual, expected) in pose.into_iter().zip(rest) {
                assert!(actual.abs_diff_eq(expected, 1e-6));
            }
        }
    }
    for (crouch, eye_height) in [(0.0, 1.6), (1.0, 1.0575)] {
        let mut pose = asset.sample_gameplay(0.0, 0.0, 0.0, crouch, None);
        View {
            id: 1,
            eye_height,
            pitch: -1.48,
        }
        .prepare_pose(&mut pose, None);
        let mut top = f32::NEG_INFINITY;
        for vertex in asset.vertices.iter().filter(|v| {
            v.material == 0 && v.joint == crate::render::avatars::character_asset::rig::CHEST
        }) {
            let point = Quat::from_rotation_x(-1.48)
                * (pose[vertex.joint].transform_point3(Vec3::from_array(vertex.position))
                    - Vec3::Y * eye_height);
            if point.z > 0.05 {
                top = top.max(point.y / (point.z * 35.0_f32.to_radians().tan()));
            }
        }
        assert!(
            top < -0.25,
            "chest obscures ground/crosshair when looking down: top={top}, crouch={crouch}"
        );
    }
}

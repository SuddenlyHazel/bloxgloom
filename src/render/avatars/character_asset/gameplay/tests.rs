use super::*;
use glam::EulerRot;

fn relative(pose: &[Mat4; JOINT_COUNT], joint: usize, asset: &CharacterAsset) -> Mat4 {
    asset.joints[joint]
        .parent
        .map_or(pose[joint], |parent| pose[parent].inverse() * pose[joint])
}

#[test]
fn named_native_joints_and_grip_hierarchy_are_stable() {
    let asset = CharacterAsset::builtin();
    for (i, name) in [
        (ROOT, "root"),
        (PELVIS, "pelvis"),
        (SPINE, "spine"),
        (CHEST, "chest"),
        (NECK, "neck"),
        (HEAD, "head"),
    ] {
        assert_eq!(asset.joints[i].name, name);
    }
    for (arm, suffix) in [(RIGHT_ARM, "R"), (LEFT_ARM, "L")] {
        for (i, prefix) in arm
            .into_iter()
            .zip(["clavicle", "upper_arm", "forearm", "hand", "grip"])
        {
            assert_eq!(asset.joints[i].name, format!("{prefix}_{suffix}"));
        }
        for pair in arm.windows(2) {
            assert_eq!(asset.joints[pair[1]].parent, Some(pair[0]));
        }
    }
}

#[test]
fn walk_and_run_articulate_all_major_joint_chains() {
    let asset = CharacterAsset::builtin();
    let rest = asset.sample("rest", 0.0);
    let expected = [PELVIS, SPINE, CHEST]
        .into_iter()
        .chain(RIGHT_ARM[..4].iter().copied())
        .chain(LEFT_ARM[..4].iter().copied())
        .chain(RIGHT_LEG[..3].iter().copied())
        .chain(LEFT_LEG[..3].iter().copied());
    for joint in expected {
        for clip in ["walk", "run"] {
            assert!(
                (0..16).any(|step| {
                    let animated = asset.sample(clip, step as f32 * 0.05);
                    !relative(&animated, joint, &asset)
                        .abs_diff_eq(relative(&rest, joint, &asset), 1e-4)
                }),
                "{clip} never articulates {}",
                asset.joints[joint].name
            );
        }
    }
    for i in 0..JOINT_COUNT {
        assert!(asset.sample("walk", 0.2)[i].abs_diff_eq(asset.sample("walk", 1.0)[i], 1e-5));
    }
    assert_ne!(asset.sample("walk", 0.2), asset.sample("run", 0.2));
}

#[test]
fn mirrored_tools_move_both_elbows_and_wrists_and_return_without_a_pop() {
    let asset = CharacterAsset::builtin();
    let rest = asset.sample("rest", 0.0);
    let mirror = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    for step in 0..=80 {
        let time = step as f32 * 0.01;
        let right = asset.sample("tool_use_right", time);
        let left = asset.sample("tool_use_left", time);
        for (r, l) in RIGHT_ARM.into_iter().zip(LEFT_ARM) {
            assert!(
                left[l].abs_diff_eq(mirror * right[r] * mirror, 1e-5),
                "tool mirroring lost {r}/{l} at {time}"
            );
        }
    }
    for clip in ["tool_use_left", "tool_use_right"] {
        let active = asset.sample(clip, 0.24);
        for arm in [RIGHT_ARM, LEFT_ARM] {
            for joint in [arm[2], arm[3]] {
                assert!(
                    !relative(&active, joint, &asset)
                        .abs_diff_eq(relative(&rest, joint, &asset), 1e-4)
                );
            }
        }
        for time in [0.0, tool_duration(true), 600.0] {
            for (actual, expected) in asset.sample(clip, time).iter().zip(rest) {
                assert!(actual.abs_diff_eq(expected, 1e-5));
            }
        }
        for (before, after) in asset.sample(clip, 0.799).iter().zip(rest) {
            assert!(before.abs_diff_eq(after, 1e-4), "tool completion pops");
        }
    }
}

#[test]
fn tools_preserve_locomotion_legs_and_crouch_can_blend_out_cleanly() {
    let asset = CharacterAsset::builtin();
    for crouch in [0.0, 0.3, 1.0] {
        let base = asset.sample_gameplay(0.35, 0.2, 1.0, crouch, None);
        for right in [false, true] {
            let active = asset.sample_gameplay(0.35, 0.2, 1.0, crouch, Some((right, 0.24)));
            for i in [ROOT, PELVIS].into_iter().chain(RIGHT_LEG).chain(LEFT_LEG) {
                assert!(active[i].abs_diff_eq(base[i], 1e-5));
            }
            let ended = asset.sample_gameplay(0.35, 0.2, 1.0, crouch, Some((right, 0.8)));
            for (a, b) in ended.into_iter().zip(base) {
                assert!(a.abs_diff_eq(b, 1e-5));
            }
        }
    }
    let base = asset.sample_gameplay(0.35, 0.2, 1.0, 0.0, None);
    let almost = asset.sample_gameplay(0.35, 0.2, 1.0, 0.00001, None);
    for (a, b) in almost.into_iter().zip(base) {
        assert!(a.abs_diff_eq(b, 1e-4));
    }
}

#[test]
fn final_head_look_clamps_animation_and_input_to_the_hair_envelope() {
    let asset = CharacterAsset::builtin();
    for yaw in [-100.0, 0.0, 100.0, f32::NAN] {
        for pitch in [-100.0, 0.0, 100.0, f32::INFINITY] {
            for right in [false, true] {
                let pose = asset.sample_gameplay_look(
                    0.5,
                    0.2,
                    1.0,
                    0.5,
                    0.4,
                    Some((right, 0.24)),
                    [yaw, pitch],
                );
                let (_, rotation, _) =
                    relative(&pose, HEAD, &asset).to_scale_rotation_translation();
                let (yaw, pitch, _) = rotation.to_euler(EulerRot::YXZ);
                assert!(yaw.abs() <= 20.0_f32.to_radians() + 1e-5);
                assert!(pitch.abs() <= 5.0_f32.to_radians() + 1e-5);
                assert!(pose.iter().all(|m| m.is_finite() && m.determinant() > 0.99));
            }
        }
    }
}

#[test]
fn grip_anchors_follow_the_animated_wrist_without_recomputing_rest_offsets() {
    let asset = CharacterAsset::builtin();
    for right in [false, true] {
        let arm = if right { RIGHT_ARM } else { LEFT_ARM };
        let bind = asset.local_pose("rest", 0.0)[arm[4]];
        let local = Mat4::from_rotation_translation(bind.rotation, bind.translation);
        for time in [0.0, 0.1, 0.24, 0.4, 0.8] {
            let pose = asset.sample_gameplay(0.3, 0.2, 1.0, 0.5, Some((right, time)));
            assert!(grip_anchor(&pose, right).abs_diff_eq(pose[arm[3]] * local, 1e-5));
        }
    }
}

#[test]
fn all_gameplay_blends_remain_finite_including_invalid_inputs() {
    let asset = CharacterAsset::builtin();
    for step in 0..=100 {
        let w = step as f32 / 100.0;
        let pose = asset.sample_gameplay_look(
            0.35,
            w * 0.8,
            w,
            1.0 - w,
            w,
            Some((true, w * 0.8)),
            [w, -w],
        );
        assert!(pose.iter().all(|m| m.is_finite() && m.determinant() > 0.99));
    }
    let pose = asset.sample_gameplay_look(
        f32::NAN,
        f32::INFINITY,
        f32::NAN,
        f32::NAN,
        f32::NAN,
        Some((true, f32::NAN)),
        [f32::NAN; 2],
    );
    assert!(pose.iter().all(|m| m.is_finite()));
}

/// Optional native pose export for the independent offline convex-SAT verifier.
/// Uses the actual Rust hierarchy, layering, head clamp and source channels.
#[test]
fn native_articulated_pose_dump() {
    let Ok(path) = std::env::var("BLOXGLOOM_POSE_DUMP") else {
        return;
    };
    let asset = CharacterAsset::builtin();
    let mut samples = Vec::new();
    let mut add = |name: String, pose: [Mat4; JOINT_COUNT]| {
        samples.push(serde_json::json!({
            "name": name,
            "matrices": pose.iter().map(|matrix| matrix.to_cols_array()).collect::<Vec<_>>()
        }));
    };
    add("rest".into(), asset.sample("rest", 0.0));
    for clip in &asset.clips {
        for step in 0..=120 {
            let t = clip.duration * step as f32 / 120.0;
            add(
                format!("source/{}/{t:.5}", clip.name),
                asset.sample(&clip.name, t),
            );
        }
    }
    for clip in [
        "idle",
        "walk",
        "run",
        "crouch",
        "tool_use_right",
        "tool_use_left",
    ] {
        let duration = if clip == "idle" { 3.0 } else { 0.8 };
        for step in 0..=80 {
            let t = duration * step as f32 / 80.0;
            add(format!("gameplay/{clip}/{t:.5}"), asset.sample(clip, t));
        }
    }
    for (gait, walk, run) in [
        ("idle", 0.0, 0.0),
        ("walk_blend", 0.5, 0.0),
        ("walk", 1.0, 0.0),
        ("jog", 1.0, 0.5),
        ("run", 1.0, 1.0),
    ] {
        for phase in [0.0, 0.2, 0.4, 0.6] {
            for crouch in [0.0, 0.5, 1.0] {
                for tool in [
                    None,
                    Some((false, 0.24)),
                    Some((true, 0.24)),
                    Some((false, 0.4)),
                    Some((true, 0.4)),
                ] {
                    for yaw in [-20.0_f32, 0.0, 20.0] {
                        for pitch in [-5.0_f32, 0.0, 5.0] {
                            add(
                                format!(
                                    "blend/{gait}/phase={phase}/crouch={crouch}/tool={tool:?}/look={yaw},{pitch}"
                                ),
                                asset.sample_gameplay_look(
                                    0.35,
                                    phase,
                                    walk,
                                    run,
                                    crouch,
                                    tool,
                                    [yaw.to_radians(), pitch.to_radians()],
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
    let payload = serde_json::json!({
        "matrix_layout": "column_major",
        "joints": asset.joints.iter().map(|joint| serde_json::json!({"name":joint.name,"parent":joint.parent})).collect::<Vec<_>>(),
        "samples": samples
    });
    let destination = std::path::PathBuf::from(path);
    let temporary = destination.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec(&payload).unwrap()).unwrap();
    std::fs::rename(temporary, destination).unwrap();
}

#[test]
fn crouch_fits_existing_gameplay_height_and_keeps_soles_planted_during_transitions() {
    let asset = CharacterAsset::builtin();
    for material in [0, 14] {
        for step in 0..=100 {
            let crouch = step as f32 / 100.0;
            let pose = asset.sample_gameplay(0.0, 0.0, 0.0, crouch, None);
            let body: Vec<_> = asset
                .vertices
                .iter()
                .filter(|v| v.material == material)
                .map(|v| {
                    (
                        v.joint,
                        pose[v.joint].transform_point3(Vec3::from_array(v.position)),
                    )
                })
                .collect();
            for foot in [RIGHT_LEG[2], LEFT_LEG[2]] {
                let bottom = body
                    .iter()
                    .filter(|(joint, _)| *joint == foot)
                    .map(|(_, point)| point.y)
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    bottom.abs() < 1e-5,
                    "foot {foot} is off ground by {bottom} at crouch={crouch}"
                );
            }
            if step == 100 {
                let height = body
                    .iter()
                    .map(|(_, point)| point.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!(
                    (1.1..=1.2).contains(&height),
                    "body {material} crouches to {height}m"
                );
            }
        }
    }
}

#[test]
fn planted_stance_and_clear_swing_feet_follow_actual_mesh_through_all_gait_blends() {
    let asset = CharacterAsset::builtin();
    let soles: Vec<_> = asset
        .vertices
        .iter()
        .filter(|vertex| {
            (vertex.material == 0 || vertex.material == 14)
                && [RIGHT_LEG[2], RIGHT_LEG[3], LEFT_LEG[2], LEFT_LEG[3]].contains(&vertex.joint)
        })
        .collect();
    for (walk, run) in [(0.0, 0.0), (0.5, 0.0), (1.0, 0.0), (1.0, 0.5), (1.0, 1.0)] {
        for crouch in [0.0, 0.5, 1.0] {
            for frame in 0..=80 {
                let time = frame as f32 * 0.01;
                let pose =
                    asset.sample_gameplay_look(0.35, time, walk, run, crouch, None, [0.0; 2]);
                let bottom = soles
                    .iter()
                    .map(|vertex| {
                        pose[vertex.joint]
                            .transform_point3(Vec3::from_array(vertex.position))
                            .y
                    })
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    bottom.abs() < 1e-5,
                    "unplanted feet at phase={time}, walk={walk}, run={run}, crouch={crouch}: {bottom}"
                );
                for knee in [RIGHT_LEG[1], LEFT_LEG[1]] {
                    let (_, rotation, _) =
                        relative(&pose, knee, &asset).to_scale_rotation_translation();
                    let flexion = rotation.to_euler(EulerRot::XYZ).0.to_degrees();
                    assert!(
                        (-179.0..=0.001).contains(&flexion),
                        "knee folded through itself: {flexion}"
                    );
                }
            }
        }
    }
}

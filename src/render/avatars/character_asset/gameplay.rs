//! Articulated gameplay poses, separate from the four preserved source tests.
//! Layers blend local transforms before evaluating the complete hierarchy.
use super::{CharacterAsset, JOINT_COUNT, LocalPose, rig::*};
use glam::{Mat4, Quat, Vec3};

const CROUCH_HIP_DEGREES: f32 = 81.0;
const LEG_LENGTH: f32 = 0.67;

pub(crate) fn tool_duration(_right: bool) -> f32 {
    0.8
}

fn weight(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn smooth(value: f32) -> f32 {
    let value = weight(value);
    value * value * (3.0 - 2.0 * value)
}

fn blend(a: LocalPose, b: LocalPose, weight: f32) -> LocalPose {
    LocalPose {
        translation: a.translation.lerp(b.translation, weight),
        rotation: a.rotation.slerp(b.rotation, weight).normalize(),
    }
}

fn overlay(base: LocalPose, rest: LocalPose, layer: LocalPose, weight: f32) -> LocalPose {
    let delta = rest.rotation.inverse() * layer.rotation;
    LocalPose {
        translation: base.translation + (layer.translation - rest.translation) * weight,
        rotation: (base.rotation * Quat::IDENTITY.slerp(delta, weight)).normalize(),
    }
}

fn rotate(pose: &mut [LocalPose; JOINT_COUNT], joint: usize, degrees: Vec3) {
    pose[joint].rotation *= Quat::from_euler(
        glam::EulerRot::XYZ,
        degrees.x.to_radians(),
        degrees.y.to_radians(),
        degrees.z.to_radians(),
    );
}

/// Applied to an initialized bind pose by `local_pose`. Source validation clips
/// still use their original tracks; gameplay clips have no old-rig channels.
pub(super) fn animate(name: &str, seconds: f32, pose: &mut [LocalPose; JOINT_COUNT]) -> bool {
    let time = if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    };
    match name {
        "idle" => {
            let breath = (time.rem_euclid(3.0) * std::f32::consts::TAU / 3.0).sin();
            pose[PELVIS].translation.y += 0.002 * breath;
            rotate(pose, PELVIS, Vec3::new(0.0, 0.0, 0.6 * breath));
            rotate(pose, SPINE, Vec3::new(0.5 * breath, 0.0, -0.5 * breath));
            rotate(pose, CHEST, Vec3::new(-0.5 * breath, 0.0, -0.1 * breath));
            for (arm, side) in [(RIGHT_ARM, 1.0), (LEFT_ARM, -1.0)] {
                rotate(pose, arm[0], Vec3::new(0.0, 0.0, side * 0.6 * breath));
                rotate(pose, arm[2], Vec3::new(1.2 * breath, 0.0, 0.0));
                rotate(pose, arm[3], Vec3::new(-0.8 * breath, 0.0, 0.0));
            }
        }
        "walk" | "run" => {
            let running = name == "run";
            let phase = time.rem_euclid(0.8) * std::f32::consts::TAU / 0.8;
            let swing = phase.sin();
            // Stance-leg shortening drives the pelvis, rather than an unrelated
            // sine bob that leaves both feet floating at maximum stride.
            let hip = swing * (if running { 38.0_f32 } else { 25.0 }).to_radians();
            pose[ROOT].translation.y -= LEG_LENGTH * (1.0 - hip.cos());
            rotate(pose, PELVIS, Vec3::new(0.0, swing * 2.5, swing * 1.2));
            rotate(
                pose,
                SPINE,
                Vec3::new(if running { -6.0 } else { -1.0 }, -swing * 2.0, -swing),
            );
            rotate(pose, CHEST, Vec3::new(1.0, -swing * 0.5, -swing * 0.2));
            rotate(
                pose,
                NECK,
                Vec3::new(if running { 5.0 } else { 0.0 }, 0.0, 0.0),
            );
            for (arm, leg, side) in [(RIGHT_ARM, RIGHT_LEG, 1.0), (LEFT_ARM, LEFT_LEG, -1.0)] {
                let stride = swing * side;
                let thigh = stride * if running { 38.0 } else { 25.0 };
                let knee = -stride.max(0.0) * if running { 95.0 } else { 65.0 };
                rotate(pose, leg[0], Vec3::new(thigh, 0.0, 0.0));
                rotate(pose, leg[1], Vec3::new(knee, 0.0, 0.0));
                rotate(pose, leg[2], Vec3::new(-thigh - knee, 0.0, 0.0));
                rotate(pose, leg[3], Vec3::new(-stride.min(0.0) * 8.0, 0.0, 0.0));
                rotate(pose, arm[0], Vec3::new(-stride, 0.0, side * stride * 1.5));
                rotate(
                    pose,
                    arm[1],
                    // The rear half opens the arm plane slightly rather than
                    // dragging a straight forearm through long hair.
                    Vec3::new(
                        if stride > 0.0 {
                            -stride * 6.0
                        } else {
                            -stride * if running { 30.0 } else { 22.0 }
                        },
                        0.0,
                        side * stride.max(0.0) * 4.0,
                    ),
                );
                rotate(
                    pose,
                    arm[2],
                    Vec3::new(
                        (if running { 24.0 } else { 8.0 })
                            + (-stride).max(0.0) * 15.0
                            + stride.max(0.0) * 22.0,
                        0.0,
                        side * stride.max(0.0) * 3.0,
                    ),
                );
                rotate(pose, arm[3], Vec3::new(stride * 4.0, 0.0, 0.0));
            }
        }
        "crouch" => {
            let crouch = smooth(time.min(0.3) / 0.3);
            // Gameplay needs a deeper squat than the preserved source test so
            // the 1.8m body fits the existing crouched collision/eye envelope.
            // Root lowering matches both leg segments at every blend value.
            pose[ROOT].translation.y -=
                LEG_LENGTH * (1.0 - (CROUCH_HIP_DEGREES.to_radians() * crouch).cos());
            for (joint, angle) in [(SPINE, -26.0), (CHEST, 3.0), (NECK, 23.0)] {
                rotate(pose, joint, Vec3::X * angle * crouch);
            }
            for (arm, leg) in [(RIGHT_ARM, RIGHT_LEG), (LEFT_ARM, LEFT_LEG)] {
                for (joint, angle) in [
                    (arm[1], 25.0),
                    (arm[2], 36.0),
                    (arm[3], -12.0),
                    (leg[0], CROUCH_HIP_DEGREES),
                    (leg[1], -2.0 * CROUCH_HIP_DEGREES),
                    (leg[2], CROUCH_HIP_DEGREES),
                ] {
                    rotate(pose, joint, Vec3::X * angle * crouch);
                }
            }
        }
        "tool_use_left" | "tool_use_right" => {
            let right = name == "tool_use_right";
            let time = time.min(tool_duration(right));
            // Rest at both ends, continuous value and velocity at each phase.
            let reach = if time < 0.24 {
                smooth(time / 0.24)
            } else {
                1.0 - smooth((time - 0.24) / 0.56)
            };
            let strike = if time < 0.38 {
                smooth(time / 0.38)
            } else {
                1.0 - smooth((time - 0.38) / 0.42)
            };
            let (arm, support, side) = if right {
                (RIGHT_ARM, LEFT_ARM, 1.0)
            } else {
                (LEFT_ARM, RIGHT_ARM, -1.0)
            };
            rotate(
                pose,
                SPINE,
                Vec3::new(-2.0 * reach, side * 3.0 * reach, 0.0),
            );
            rotate(
                pose,
                CHEST,
                Vec3::new(2.0 * reach, -side * 3.0 * reach, 0.0),
            );
            rotate(pose, HEAD, Vec3::new(0.0, -side * 6.0 * reach, 0.0));
            rotate(pose, arm[0], Vec3::new(0.0, 0.0, side * 3.0 * reach));
            rotate(pose, arm[1], Vec3::X * (40.0 * reach));
            rotate(pose, arm[2], Vec3::X * (55.0 * reach - 20.0 * strike));
            rotate(
                pose,
                arm[3],
                Vec3::new(-15.0 * strike, side * 5.0 * reach, 0.0),
            );
            rotate(pose, support[0], Vec3::new(0.0, 0.0, -side * reach));
            rotate(pose, support[1], Vec3::X * (4.0 * reach));
            rotate(pose, support[2], Vec3::X * (15.0 * reach));
            rotate(pose, support[3], Vec3::X * (-3.0 * strike));
        }
        _ => return false,
    }
    true
}

impl CharacterAsset {
    #[cfg(test)]
    pub fn sample_gameplay(
        &self,
        idle_time: f32,
        walk_time: f32,
        walk_weight: f32,
        crouch_weight: f32,
        tool: Option<(bool, f32)>,
    ) -> [Mat4; JOINT_COUNT] {
        self.sample_gameplay_look(
            idle_time,
            walk_time,
            walk_weight,
            0.0,
            crouch_weight,
            tool,
            [0.0; 2],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sample_gameplay_look(
        &self,
        idle_time: f32,
        walk_time: f32,
        walk_weight: f32,
        run_weight: f32,
        crouch_weight: f32,
        tool: Option<(bool, f32)>,
        look: [f32; 2],
    ) -> [Mat4; JOINT_COUNT] {
        let idle = self.local_pose("idle", idle_time);
        let walk = self.local_pose("walk", walk_time);
        let walk_weight = weight(walk_weight);
        let crouch_weight = weight(crouch_weight);
        let run_weight = weight(run_weight) * (1.0 - crouch_weight);
        let mut locomotion = walk;
        if run_weight > 0.0 {
            let run = self.local_pose("run", walk_time);
            locomotion = std::array::from_fn(|i| blend(walk[i], run[i], run_weight));
        }
        let mut pose = std::array::from_fn(|i| blend(idle[i], locomotion[i], walk_weight));
        let rest = self.local_pose("rest", 0.0);
        if crouch_weight > 0.0 {
            let crouch = self.local_pose("crouch", 0.3);
            for i in 0..JOINT_COUNT {
                // Shorter strides keep the already bent knees anatomically safe.
                let shortening = if RIGHT_LEG.contains(&i) || LEFT_LEG.contains(&i) {
                    0.8
                } else {
                    0.65
                };
                pose[i] = blend(idle[i], pose[i], 1.0 - shortening * crouch_weight);
                pose[i] = overlay(pose[i], rest[i], crouch[i], crouch_weight);
            }
            // Rotating both leg segments shortens them nonlinearly. A linear
            // root lerp buries the soles halfway through an otherwise smooth
            // crouch; match the blended hip/knee angle instead.
            let angle = CROUCH_HIP_DEGREES.to_radians();
            pose[ROOT].translation.y += LEG_LENGTH
                * ((1.0 - angle.cos()) * crouch_weight - (1.0 - (angle * crouch_weight).cos()));
        }
        if let Some((right, seconds)) = tool {
            let tool = self.local_pose(
                if right {
                    "tool_use_right"
                } else {
                    "tool_use_left"
                },
                seconds,
            );
            for i in [SPINE, CHEST, NECK, HEAD]
                .into_iter()
                .chain(RIGHT_ARM)
                .chain(LEFT_ARM)
            {
                pose[i] = overlay(pose[i], rest[i], tool[i], 1.0);
            }
        }
        apply_look(&mut pose, look);
        let mut matrices = self.matrices(pose);
        ground(&mut matrices);
        matrices
    }
}

/// These are the native shoe's actual flat sole vertices, shared by both body
/// types. Checking sixteen support points is bounded, independent of mesh size.
/// A final rigid translation handles pelvis roll and blended poses as well as
/// the analytic stance height; neither it nor the socket transforms affect the
/// authoritative actor origin or collision body. Source clips bypass this.
pub(super) fn ground(pose: &mut [Mat4; JOINT_COUNT]) {
    let mut lowest = f32::INFINITY;
    for leg in [RIGHT_LEG, LEFT_LEG] {
        for (joint, half_width, y, front, back) in [
            (leg[2], 0.1005, -0.1, -0.137, 0.097),
            (leg[3], 0.1015, -0.075, -0.114, -0.046),
        ] {
            for x in [-half_width, half_width] {
                for z in [front, back] {
                    lowest = lowest.min(pose[joint].transform_point3(Vec3::new(x, y, z)).y);
                }
            }
        }
    }
    for matrix in pose {
        matrix.w_axis.y -= lowest;
    }
}

#[cfg(test)]
mod tests;

//! Presentation-only perspective and swept camera clearance against installed cells.
use super::Camera;
use glam::Vec3;

const DISTANCE: f32 = 4.0;
const RADIUS: f32 = 0.2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Perspective {
    #[default]
    FirstPerson,
    Behind,
    Front,
}

impl Perspective {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::FirstPerson => Self::Behind,
            Self::Behind => Self::Front,
            Self::Front => Self::FirstPerson,
        }
    }

    pub(crate) fn view(self, eye: Camera, mut blocked: impl FnMut([i32; 3]) -> bool) -> Camera {
        if self == Self::FirstPerson {
            return eye;
        }
        let direction = eye.direction() * if self == Self::Behind { -1.0 } else { 1.0 };
        let desired = eye.position + direction * DISTANCE;
        let min = (eye.position.min(desired) - Vec3::splat(RADIUS))
            .floor()
            .as_ivec3();
        let max = (eye.position.max(desired) + Vec3::splat(RADIUS))
            .floor()
            .as_ivec3();
        let mut distance = DISTANCE;
        // Four-block boom: bounded local sweep, including the camera volume at corners.
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                for z in min.z..=max.z {
                    let cell = Vec3::new(x as f32, y as f32, z as f32);
                    if let Some(hit) = intersection(eye.position, direction, cell)
                        && hit < distance
                        && blocked([x, y, z])
                    {
                        distance = (hit - 0.01).max(0.0);
                    }
                }
            }
        }
        Camera {
            position: eye.position + direction * distance,
            yaw: eye.yaw
                + if self == Self::Front {
                    std::f32::consts::PI
                } else {
                    0.0
                },
            pitch: if self == Self::Front {
                -eye.pitch
            } else {
                eye.pitch
            },
            ..eye
        }
    }
}

fn intersection(origin: Vec3, direction: Vec3, cell: Vec3) -> Option<f32> {
    let min = cell - Vec3::splat(RADIUS);
    let max = cell + Vec3::splat(1.0 + RADIUS);
    let mut near: f32 = 0.0;
    let mut far: f32 = DISTANCE;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-6 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    Some(near)
}

#[cfg(test)]
mod tests;

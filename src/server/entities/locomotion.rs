//! Shared deterministic ground locomotion. AI supplies a horizontal intention;
//! collision and gravity run independently, against the worker's fenced view.
use super::EntityError;
use crate::server::voxel_view::VoxelView;

pub(super) const DT: f32 = 0.04; // two 50 Hz logical ticks, never wall time

#[derive(Clone, Copy)]
pub(super) struct Body {
    pub half_width: f32,
    pub height: f32,
    pub speed: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Movement {
    pub position: [f32; 3],
    pub vertical_velocity: f32,
    pub grounded: bool,
}

pub(super) fn valid_position(position: [f32; 3]) -> bool {
    position
        .iter()
        .all(|v| v.is_finite() && v.abs() < 999_999.0)
        && position[1] > crate::world::BEDROCK_Y as f32
}

impl Body {
    pub fn clear(self, view: &VoxelView, p: [f32; 3]) -> Result<bool, EntityError> {
        self.volume_clear(
            view,
            [
                p[0] - self.half_width,
                p[1] + 0.0001,
                p[2] - self.half_width,
            ],
            [
                p[0] + self.half_width - 0.0001,
                p[1] + self.height - 0.0001,
                p[2] + self.half_width - 0.0001,
            ],
        )
    }

    fn volume_clear(
        self,
        view: &VoxelView,
        min: [f32; 3],
        max: [f32; 3],
    ) -> Result<bool, EntityError> {
        for x in min[0].floor() as i32..=max[0].floor() as i32 {
            for y in min[1].floor() as i32..=max[1].floor() as i32 {
                for z in min[2].floor() as i32..=max[2].floor() as i32 {
                    if view
                        .is_solid(x, y, z)
                        .map_err(|_| EntityError::ViewOutOfRange)?
                    {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    pub fn grounded(self, view: &VoxelView, p: [f32; 3]) -> Result<bool, EntityError> {
        let mut below = p;
        below[1] -= 0.002;
        Ok(!self.clear(view, below)?)
    }

    /// Navigation requires the entire footprint to have support, unlike physics
    /// where partial support suffices. Never deliberately walk off a ledge.
    pub fn supported(self, view: &VoxelView, p: [f32; 3]) -> Result<bool, EntityError> {
        for x in (p[0] - self.half_width).floor() as i32
            ..=(p[0] + self.half_width - 0.0001).floor() as i32
        {
            for z in (p[2] - self.half_width).floor() as i32
                ..=(p[2] + self.half_width - 0.0001).floor() as i32
            {
                if !view
                    .is_solid(x, (p[1] - 0.002).floor() as i32, z)
                    .map_err(|_| EntityError::ViewOutOfRange)?
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Shared by the path graph and its follower, including the space between
    /// nodes. Short subdivisions prevent corner cutting and missed thin walls.
    pub fn walk_edge(
        self,
        view: &VoxelView,
        from: [f32; 3],
        to: [f32; 3],
    ) -> Result<bool, EntityError> {
        if !valid_position(from) || !valid_position(to) {
            return Ok(false);
        }
        let distance = (to[0] - from[0]).abs().max((to[2] - from[2]).abs());
        if distance > 1.51 || from[1] != to[1] {
            return Ok(false);
        }
        let steps = (distance / 0.1).ceil().max(1.0) as usize;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let p = std::array::from_fn(|axis| from[axis] + (to[axis] - from[axis]) * t);
            if !self.clear(view, p)? || !self.supported(view, p)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn advance(
        self,
        view: &VoxelView,
        position: [f32; 3],
        velocity: f32,
        target: Option<[f32; 3]>,
    ) -> Result<Movement, EntityError> {
        if !valid_position(position) || !(-24.0..=0.0).contains(&velocity) {
            return Err(EntityError::InvalidLocation);
        }
        let mut result = Movement {
            position,
            vertical_velocity: 0.0,
            grounded: false,
        };
        // An edit can embed an actor. Do not tunnel out of a new solid block.
        if !self.clear(view, position)? {
            return Ok(result);
        }
        result.grounded = self.grounded(view, position)?;
        if result.grounded {
            if let Some(target) = target {
                let delta = glam::Vec2::new(target[0] - position[0], target[2] - position[2]);
                let step = delta.clamp_length_max(self.speed * DT);
                let next = [position[0] + step.x, position[1], position[2] + step.y];
                if self.walk_edge(view, position, next)? {
                    result.position = next;
                }
            }
            return Ok(result);
        }
        result.vertical_velocity = (velocity - 20.0 * DT).max(-24.0);
        let distance = result.vertical_velocity * DT;
        let steps = (distance.abs() / 0.1).ceil().max(1.0) as usize;
        for _ in 0..steps {
            let mut next = result.position;
            next[1] += distance / steps as f32;
            if !valid_position(next) {
                return Err(EntityError::InvalidLocation);
            }
            if !self.clear(view, next)? {
                // Downward collision against a voxel top. The < 0.1 step cannot
                // cross an entire voxel; snap exactly, avoiding float hovering.
                result.position[1] = next[1].ceil();
                result.vertical_velocity = 0.0;
                result.grounded = true;
                break;
            }
            result.position = next;
        }
        Ok(result)
    }
}

#[cfg(test)]
pub(super) mod tests;

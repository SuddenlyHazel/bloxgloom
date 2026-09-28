//! Pure, bounded falling-entity decision over an authoritative captured column.
//! The host supplies solid-cell reads and stages the returned plan through its
//! existing entity transaction. An unavailable cell never means empty space.
use super::Error;

/// Read only the captured, authoritative terrain for a falling decision.
/// Return `OutOfRange` for unavailable cells; never substitute fallback terrain.
pub trait FallingWorld {
    fn solid(&self, cell: [i32; 3]) -> Result<bool, Error>;
}

/// One fixed-step falling decision. Callers persist the returned motion and
/// schedule through their normal entity transaction, not inside this helper.
#[derive(Clone, Copy)]
pub struct FallingContext<'a> {
    pub position: [f32; 3],
    pub vertical_speed: f32,
    pub suspended: bool,
    pub tick: u64,
    pub step_seconds: f32,
    pub gravity: f32,
    pub terminal_speed: f32,
    pub radius: f32,
    pub world: &'a dyn FallingWorld,
}

/// Optional updates to the entity's persisted motion; `None` means unchanged.
/// An absent `next_tick` suspends ticking until the host wakes the entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FallingPlan {
    pub position: Option<[f32; 3]>,
    pub vertical_speed: Option<f32>,
    pub next_tick: Option<u64>,
}

impl FallingContext<'_> {
    pub fn plan(&self) -> Result<FallingPlan, Error> {
        if self.position.iter().any(|value| !value.is_finite())
            || !self.vertical_speed.is_finite()
            || !self.step_seconds.is_finite()
            || self.step_seconds <= 0.0
            || !self.gravity.is_finite()
            || self.gravity < 0.0
            || !self.terminal_speed.is_finite()
            || self.terminal_speed < 0.0
            || !self.radius.is_finite()
            || self.radius <= 0.0
        {
            return Err(Error::InvalidState);
        }
        let [x, y, z] = self.position;
        if self.suspended {
            // Settled drops check the supporting layer, not the inclusive
            // start of the active sweep (which may contain the drop itself).
            let support = y.floor() - 1.0;
            if self.first_solid_top(support, support)?.is_some() {
                return Ok(FallingPlan {
                    position: None,
                    vertical_speed: None,
                    next_tick: None,
                });
            }
        }
        let dt = self.step_seconds.min(0.1);
        let speed = (self.vertical_speed - self.gravity * dt).max(-self.terminal_speed);
        let start = y - self.radius;
        let end = start + speed * dt;
        if let Some(top) = self.first_solid_top(start, end)? {
            let rest = top + self.radius;
            Ok(FallingPlan {
                position: (y.to_bits() != rest.to_bits()).then_some([x, rest, z]),
                vertical_speed: (self.vertical_speed.to_bits() != 0.0_f32.to_bits()).then_some(0.0),
                next_tick: None,
            })
        } else {
            Ok(FallingPlan {
                position: Some([x, end + self.radius, z]),
                vertical_speed: (speed.to_bits() != self.vertical_speed.to_bits()).then_some(speed),
                next_tick: Some(self.tick.checked_add(1).ok_or(Error::Exhausted)?),
            })
        }
    }

    fn first_solid_top(&self, start: f32, end: f32) -> Result<Option<f32>, Error> {
        // A caller-controlled speed/radius must not turn a single scheduled
        // entity into an unbounded column scan. Stock drops visit at most a
        // handful of layers per fixed step.
        let bottom = end.floor() as i32;
        let top = start.floor() as i32;
        if i64::from(top) - i64::from(bottom) > 64 {
            return Err(Error::Exhausted);
        }
        let mut hit: Option<f32> = None;
        for y in (bottom..=top).rev() {
            let top = y as f32 + 1.0;
            if top < end {
                continue;
            }
            for x in [
                self.position[0] - self.radius,
                self.position[0] + self.radius,
            ] {
                for z in [
                    self.position[2] - self.radius,
                    self.position[2] + self.radius,
                ] {
                    if self.world.solid([x.floor() as i32, y, z.floor() as i32])? {
                        hit = Some(hit.map_or(top, |previous| previous.max(top)));
                    }
                }
            }
        }
        Ok(hit)
    }
}

#[cfg(test)]
mod tests;

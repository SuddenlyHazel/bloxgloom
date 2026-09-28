//! Player body geometry shared by authoritative movement, spawn selection,
//! placement checks, and client prediction. This is a builtin contract, not a
//! runtime tuning knob: changing it requires a coordinated client/server update.

/// Feet-relative collision shape. Sampling heights preserve the built-in
/// voxel movement behavior; overlap uses the same outer bounds for placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub half_width: f32,
    pub foot_inset: f32,
    pub middle_height: f32,
    pub head_height: f32,
}

pub const BUILTIN_BODY: Body = Body {
    half_width: 0.3,
    foot_inset: 0.05,
    middle_height: 0.9,
    head_height: 1.75,
};

/// Fixed, shared movement rates. The server budget is deliberately above
/// predicted input speed to tolerate transport timing; it remains an
/// authoritative admission limit, not a client-requested rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionRates {
    pub intent_blocks_per_second: f32,
    pub budget_blocks_per_second: f64,
}

pub const BUILTIN_MOTION: MotionRates = MotionRates {
    intent_blocks_per_second: 8.0,
    budget_blocks_per_second: 10.0,
};

/// Fixed builtin surface-search order for startup and later joins. The host
/// still reads authoritative terrain, tests collision and requests missing
/// chunks; this policy never supplies procedural fallback or chooses a world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnSearch {
    pub headroom: i32,
    pub max_rise: i32,
}

pub const BUILTIN_SPAWN: SpawnSearch = SpawnSearch {
    headroom: 32,
    max_rise: 128,
};

impl SpawnSearch {
    pub fn ceiling(self, max_generated_height: i32) -> i32 {
        max_generated_height.saturating_add(self.headroom)
    }

    /// Startup prefers the highest safe origin surface, including negative Y.
    pub fn startup_support_levels(
        self,
        bedrock_y: i32,
        max_generated_height: i32,
    ) -> impl Iterator<Item = i32> {
        (bedrock_y..self.ceiling(max_generated_height)).rev()
    }

    /// Returning players first try the original or higher surface, then the
    /// excavated lower surface. Missing cached terrain is never guessed as air;
    /// the host may still accept a later known-safe surface.
    pub fn cached_feet_levels(self, anchor_y: i32, bedrock_y: i32) -> impl Iterator<Item = i32> {
        (anchor_y..anchor_y.saturating_add(self.max_rise))
            .chain((bedrock_y.saturating_add(1)..anchor_y).rev())
    }

    pub fn feet(self, feet_y: i32) -> [f32; 3] {
        [0.5, feet_y as f32, 0.5]
    }
}

impl Body {
    /// Test the same twelve voxel samples used for movement and spawn checks.
    pub fn collides<E>(
        self,
        feet: [f32; 3],
        mut solid: impl FnMut(i32, i32, i32) -> Result<bool, E>,
    ) -> Result<bool, E> {
        for x in [feet[0] - self.half_width, feet[0] + self.half_width] {
            for y in [
                feet[1] + self.foot_inset,
                feet[1] + self.middle_height,
                feet[1] + self.head_height,
            ] {
                for z in [feet[2] - self.half_width, feet[2] + self.half_width] {
                    if solid(x.floor() as i32, y.floor() as i32, z.floor() as i32)? {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    /// True if a unit block would intersect the player's collision bounds.
    pub fn intersects_block(self, block: [i32; 3], feet: [f32; 3]) -> bool {
        let [x, y, z] = block.map(|n| n as f32);
        x < feet[0] + self.half_width
            && x + 1.0 > feet[0] - self.half_width
            && y < feet[1] + self.head_height
            && y + 1.0 > feet[1] + self.foot_inset
            && z < feet[2] + self.half_width
            && z + 1.0 > feet[2] - self.half_width
    }
}

#[cfg(test)]
#[path = "player/tests.rs"]
mod tests;

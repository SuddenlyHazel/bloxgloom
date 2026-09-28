//! Player body geometry shared by authoritative movement, spawn selection,
//! placement checks, and client prediction. Rules are validated immutable values;
//! the host freezes the selected contract before opening a world.

/// One immutable player contract. Accessors return copies, never mutable fields.
/// Validation preserves the bounds assumed by the twelve-sample collision shape,
/// fixed-point movement accounting, and 3x3x3 movement snapshot capture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerRules {
    body: Body,
    motion: MotionRates,
    spawn: SpawnSearch,
    eye_height: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidPlayerRules {
    Body,
    Motion,
    Spawn,
    EyeHeight,
}

pub const BUILTIN_RULES: PlayerRules = PlayerRules {
    body: BUILTIN_BODY,
    motion: BUILTIN_MOTION,
    spawn: BUILTIN_SPAWN,
    eye_height: 1.6,
};

impl PlayerRules {
    /// Canonical fixed-size representation used by package negotiation and save
    /// identity. Includes every field, without platform-dependent padding.
    pub fn canonical_bytes(self) -> [u8; 40] {
        let mut bytes = [0; 40];
        for (index, value) in [
            self.body.half_width,
            self.body.foot_inset,
            self.body.middle_height,
            self.body.head_height,
            self.motion.intent_blocks_per_second,
            self.eye_height,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[24..32].copy_from_slice(&self.motion.budget_blocks_per_second.to_le_bytes());
        bytes[32..36].copy_from_slice(&self.spawn.headroom.to_le_bytes());
        bytes[36..40].copy_from_slice(&self.spawn.max_rise.to_le_bytes());
        bytes
    }

    pub fn from_canonical_bytes(bytes: [u8; 40]) -> Result<Self, InvalidPlayerRules> {
        let float = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        Self::new(
            Body {
                half_width: float(0),
                foot_inset: float(4),
                middle_height: float(8),
                head_height: float(12),
            },
            MotionRates {
                intent_blocks_per_second: float(16),
                budget_blocks_per_second: f64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            },
            SpawnSearch {
                headroom: i32::from_le_bytes(bytes[32..36].try_into().unwrap()),
                max_rise: i32::from_le_bytes(bytes[36..40].try_into().unwrap()),
            },
            float(20),
        )
    }

    pub fn new(
        body: Body,
        motion: MotionRates,
        spawn: SpawnSearch,
        eye_height: f32,
    ) -> Result<Self, InvalidPlayerRules> {
        let rules = Self {
            body,
            motion,
            spawn,
            eye_height,
        };
        rules.validate()?;
        Ok(rules)
    }

    pub fn validate(self) -> Result<(), InvalidPlayerRules> {
        let body = self.body;
        if [
            body.half_width,
            body.foot_inset,
            body.middle_height,
            body.head_height,
        ]
        .iter()
        .any(|value| !value.is_finite())
            || !(0.125..=0.5).contains(&body.half_width)
            || !(0.0..=0.25).contains(&body.foot_inset)
            || body.middle_height <= body.foot_inset
            || body.head_height <= body.middle_height
            || body.middle_height - body.foot_inset > 1.0
            || body.head_height - body.middle_height > 1.0
            || body.head_height > 2.0
        {
            return Err(InvalidPlayerRules::Body);
        }
        let motion = self.motion;
        // 16 blocks/s permits a 4-block burst: below u32 nanoblock capacity,
        // the resolver's 64 steps/axis, and the neighboring-chunk capture bound.
        if !motion.intent_blocks_per_second.is_finite()
            || !motion.budget_blocks_per_second.is_finite()
            || motion.intent_blocks_per_second < 0.1
            || f64::from(motion.intent_blocks_per_second) > motion.budget_blocks_per_second
            || motion.budget_blocks_per_second > 16.0
        {
            return Err(InvalidPlayerRules::Motion);
        }
        if !(2..=128).contains(&self.spawn.headroom) || !(1..=1024).contains(&self.spawn.max_rise) {
            return Err(InvalidPlayerRules::Spawn);
        }
        if !self.eye_height.is_finite()
            || self.eye_height < body.foot_inset
            || self.eye_height > body.head_height
        {
            return Err(InvalidPlayerRules::EyeHeight);
        }
        Ok(())
    }

    pub const fn body(self) -> Body {
        self.body
    }
    pub const fn motion(self) -> MotionRates {
        self.motion
    }
    pub const fn spawn(self) -> SpawnSearch {
        self.spawn
    }
    pub const fn eye_height(self) -> f32 {
        self.eye_height
    }
}

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

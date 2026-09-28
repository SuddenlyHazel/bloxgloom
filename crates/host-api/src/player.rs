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

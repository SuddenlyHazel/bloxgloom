//! Bounded presentation summaries. Missing coverage is never interpreted as air.

use crate::content::{BlockStateId, Catalog};

mod extract;
mod reduce;
pub(crate) mod skylight;
#[cfg(test)]
mod tests;
pub use extract::extract;
pub(crate) use reduce::merge as merge_columns;
pub use reduce::reduce_parent;

pub const TILE_SIZE: usize = 32;
pub const TILE_COLUMNS: usize = TILE_SIZE * TILE_SIZE;
pub const MAX_LEVEL: u8 = 20;
pub const MAX_SPANS_PER_COLUMN: usize = 32;
pub const MAX_TILE_SPANS: usize = 3072;
pub const MAX_TILE_COVERAGE: usize = 2048;
pub const MAX_TILE_BYTES: usize = 60 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileKey {
    pub level: u8,
    pub x: i32,
    pub z: i32,
}

impl TileKey {
    pub fn sample_width(self) -> Option<i32> {
        (self.level <= MAX_LEVEL).then(|| 1_i32 << self.level)
    }
    pub fn footprint(self) -> Option<i32> {
        self.sample_width()?.checked_mul(TILE_SIZE as i32)
    }
    /// Half-open [min x, min z, max x, max z].
    pub fn bounds(self) -> Option<[i32; 4]> {
        let width = self.footprint()?;
        let x = self.x.checked_mul(width)?;
        let z = self.z.checked_mul(width)?;
        Some([x, z, x.checked_add(width)?, z.checked_add(width)?])
    }
    pub fn containing(level: u8, x: i32, z: i32) -> Option<Self> {
        let width = Self { level, x: 0, z: 0 }.footprint()?;
        let key = Self {
            level,
            x: x.div_euclid(width),
            z: z.div_euclid(width),
        };
        key.bounds()?;
        Some(key)
    }
    pub fn parent(self) -> Option<Self> {
        let key = Self {
            level: self.level.checked_add(1)?,
            x: self.x.div_euclid(2),
            z: self.z.div_euclid(2),
        };
        key.bounds()?;
        Some(key)
    }
    pub fn children(self) -> Option<[Self; 4]> {
        let level = self.level.checked_sub(1)?;
        let x = self.x.checked_mul(2)?;
        let z = self.z.checked_mul(2)?;
        let keys = [
            Self { level, x, z },
            Self {
                level,
                x: x.checked_add(1)?,
                z,
            },
            Self {
                level,
                x,
                z: z.checked_add(1)?,
            },
            Self {
                level,
                x: x.checked_add(1)?,
                z: z.checked_add(1)?,
            },
        ];
        for key in keys {
            key.bounds()?;
        }
        Some(keys)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    pub bottom: i32,
    pub top: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub bottom: i32,
    pub top: i32,
    pub state: BlockStateId,
    pub sky: u8,
    pub glow: u8,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Column {
    pub coverage: Vec<Interval>,
    pub spans: Vec<Span>,
}
impl Column {
    pub fn known(&self, bottom: i32, top: i32) -> bool {
        bottom < top
            && self
                .coverage
                .iter()
                .any(|v| v.bottom <= bottom && v.top >= top)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LodTile {
    pub key: TileKey,
    /// Publication revision; dependency generations belong to the owning scheduler.
    pub revision: u64,
    pub columns: Vec<Column>,
    pub geometric_error: u32,
}
impl LodTile {
    /// Canonical uncompressed wire size including conservative tile header allowance.
    pub fn encoded_bytes(&self) -> usize {
        32 + self
            .columns
            .iter()
            .map(|c| 4 + c.coverage.len() * 8 + c.spans.len() * 14)
            .sum::<usize>()
    }
    pub fn validate(&self, catalog: &Catalog) -> Result<(), String> {
        self.validate_structure(catalog)?;
        let spans: usize = self.columns.iter().map(|c| c.spans.len()).sum();
        let coverage: usize = self.columns.iter().map(|c| c.coverage.len()).sum();
        if self
            .columns
            .iter()
            .any(|c| c.spans.len() > MAX_SPANS_PER_COLUMN)
        {
            return Err("LOD column exceeds span budget".into());
        }
        if spans > MAX_TILE_SPANS
            || coverage > MAX_TILE_COVERAGE
            || self.encoded_bytes() > MAX_TILE_BYTES
        {
            return Err("LOD tile exceeds payload budget".into());
        }
        Ok(())
    }

    /// Fit a render summary by discarding internal material boundaries only.
    /// Accurate occupancy and all unknown/known intervals are preserved. If
    /// these alone exceed admission caps, callers publish unavailable instead.
    pub(crate) fn into_render_summary(mut self, catalog: &Catalog) -> Result<Self, String> {
        self.validate_structure(catalog)?;
        if self.validate(catalog).is_err() {
            for column in &mut self.columns {
                let mut spans: Vec<Span> = Vec::new();
                for s in &column.spans {
                    if let Some(last) = spans.last_mut()
                        && last.top == s.bottom
                        && (catalog.block_flags(last.state) ^ catalog.block_flags(s.state))
                            & crate::content::FLUID
                            == 0
                    {
                        last.top = s.top;
                        last.state = s.state;
                        last.sky = s.sky;
                        last.glow = last.glow.max(s.glow);
                    } else {
                        spans.push(*s);
                    }
                }
                column.spans = spans;
            }
        }
        self.validate(catalog)?;
        Ok(self)
    }

    fn validate_structure(&self, catalog: &Catalog) -> Result<(), String> {
        if self.key.bounds().is_none() || self.columns.len() != TILE_COLUMNS {
            return Err("invalid LOD tile bounds or column count".into());
        }
        for c in &self.columns {
            if c.coverage.iter().any(|v| v.bottom >= v.top)
                || c.coverage.windows(2).any(|v| v[0].top >= v[1].bottom)
            {
                return Err("invalid LOD coverage intervals".into());
            }
            if c.spans.windows(2).any(|v| v[0].top > v[1].bottom) {
                return Err("overlapping LOD spans".into());
            }
            for s in &c.spans {
                let Some(state) = catalog.state(s.state) else {
                    return Err("unknown LOD material".into());
                };
                if s.bottom >= s.top
                    || s.top.checked_sub(s.bottom).is_none()
                    || !c.known(s.bottom, s.top)
                    || s.sky > 15
                    || s.glow > 15
                    || state.flags
                        & (crate::content::OPAQUE | crate::content::CUTOUT | crate::content::FLUID)
                        == 0
                    || state.flags & crate::content::PLANT != 0
                {
                    return Err("invalid LOD span".into());
                }
            }
        }
        Ok(())
    }
}

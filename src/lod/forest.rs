//! Server-authored botanical presentation, independent of coarse column width.
use super::TileKey;
use crate::content::{BlockStateId, CUTOUT, Catalog, OPAQUE};

pub const MAX_TREE_FEATURES: usize = 2048;
/// This is a presentation descriptor, never client-authoritative world state.
/// Shape contains the source start direction and three source bough height bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TreeFeature {
    pub anchor: [i32; 3],
    pub trunk_height: u8,
    /// Server-certified coarse exterior support; None omits only this proxy.
    pub support_y: Option<i16>,
    pub log: BlockStateId,
    pub leaves: BlockStateId,
    pub branch_x: BlockStateId,
    pub branch_z: BlockStateId,
    pub species: u8,
    pub shape: u8,
}
impl TreeFeature {
    pub const WIRE_BYTES: usize = 27;
    pub(crate) fn support_in_column(
        self,
        column: &super::Column,
        catalog: &Catalog,
    ) -> Option<i16> {
        let root = self.anchor[1] + 1;
        let surface = column
            .spans
            .iter()
            .rev()
            .find(|s| catalog.block_flags(s.state) & (OPAQUE | crate::content::FLUID) != 0)?;
        if catalog.block_flags(surface.state) & crate::content::FLUID != 0 {
            return None;
        }
        if surface.top >= root {
            // A sampled roof/plateau may cover the source stem. Do not join its
            // cave floor; keep only the original stem if its root is known.
            return column
                .known(root - 1, root + 1)
                .then(|| i16::try_from(root).ok())
                .flatten();
        }
        if root - surface.top > 3 || !column.known(surface.top, root + 1) {
            return None;
        }
        i16::try_from(surface.top).ok()
    }
    pub(crate) fn bounds(self) -> Option<([i32; 3], [i32; 3])> {
        Some((
            [
                self.anchor[0].checked_sub(6)?,
                self.anchor[1].checked_add(1)?,
                self.anchor[2].checked_sub(6)?,
            ],
            [
                self.anchor[0].checked_add(7)?,
                self.anchor[1].checked_add(i32::from(self.trunk_height) + 3)?,
                self.anchor[2].checked_add(7)?,
            ],
        ))
    }
    pub(crate) fn column_bounds(self, key: TileKey) -> Option<[usize; 4]> {
        let [x, z, mx, mz] = key.bounds()?;
        let width = i64::from(key.sample_width()?);
        let (min, max) = self.bounds()?;
        if min[0] >= mx || max[0] <= x || min[2] >= mz || max[2] <= z {
            return None;
        }
        let first = |value: i32, origin: i32| {
            (i64::from(value) - i64::from(origin))
                .div_euclid(width)
                .clamp(0, 31) as usize
        };
        let last = |value: i32, origin: i32| {
            ((i64::from(value) - 1 - i64::from(origin)).div_euclid(width) + 1).clamp(1, 32) as usize
        };
        Some([
            first(min[0], x),
            first(min[2], z),
            last(max[0], x),
            last(max[2], z),
        ])
    }
    pub(crate) fn validate(self, key: TileKey, catalog: &Catalog) -> Result<(), String> {
        let [x, z, mx, mz] = key.bounds().ok_or("invalid forest tile bounds")?;
        let (min, max) = self.bounds().ok_or("forest coordinate overflow")?;
        if !(4..=24).contains(&self.trunk_height)
            || self.species > 9
            || self.shape > 31
            || self.support_y.is_some_and(|y| {
                let difference = self.anchor[1] + 1 - i32::from(y);
                !(0..=3).contains(&difference)
            })
            || max[0] <= x
            || min[0] >= mx
            || max[2] <= z
            || min[2] >= mz
            || i16::try_from(self.anchor[0] - x).is_err()
            || i16::try_from(self.anchor[2] - z).is_err()
            || i16::try_from(self.anchor[1]).is_err()
        {
            return Err("invalid forest geometry descriptor".into());
        }
        for material in [self.log, self.branch_x, self.branch_z] {
            if catalog
                .state(material)
                .is_none_or(|state| state.flags & OPAQUE == 0)
            {
                return Err("invalid forest bark material".into());
            }
        }
        if catalog
            .state(self.leaves)
            .is_none_or(|state| state.flags & CUTOUT == 0)
        {
            return Err("invalid forest leaf material".into());
        }
        Ok(())
    }
}

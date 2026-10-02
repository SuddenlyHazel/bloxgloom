//! Drawable transition proof, independent of triangle count and GPU handles.
use crate::lod::{Interval, LodTile};
#[derive(Clone, Debug)]
pub(super) struct Coverage {
    pub intervals: Vec<Vec<Interval>>,
    pub occupied: Vec<Vec<Interval>>,
}
impl Coverage {
    pub(super) fn from_tile(tile: &LodTile) -> Self {
        Self {
            intervals: tile.columns.iter().map(|c| c.coverage.clone()).collect(),
            occupied: tile
                .columns
                .iter()
                .map(|c| {
                    c.spans
                        .iter()
                        .map(|s| Interval {
                            bottom: s.bottom,
                            top: s.top,
                        })
                        .collect()
                })
                .collect(),
        }
    }
}
/// A parent's material can disappear only after all four corresponding fine
/// columns examined its vertical interval. Unknown coverage cannot count as air.
pub(super) fn can_refine(parent: &Coverage, children: [&Coverage; 4]) -> bool {
    for z in 0..32 {
        for x in 0..32 {
            let child = children[x / 16 + 2 * (z / 16)];
            let xx = (x % 16) * 2;
            let zz = (z % 16) * 2;
            for span in &parent.occupied[x + 32 * z] {
                for index in [
                    xx + 32 * zz,
                    xx + 1 + 32 * zz,
                    xx + 32 * (zz + 1),
                    xx + 1 + 32 * (zz + 1),
                ] {
                    if !child.intervals[index]
                        .iter()
                        .any(|c| c.bottom <= span.bottom && c.top >= span.top)
                    {
                        return false;
                    }
                }
            }
        }
    }
    true
}

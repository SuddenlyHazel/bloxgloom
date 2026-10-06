//! Drawable transition proof, independent of triangle count and GPU handles.
use crate::lod::{Interval, LodTile};
#[derive(Clone, Debug)]
pub(super) struct Coverage {
    pub intervals: Vec<Vec<Interval>>,
    pub occupied: Vec<Vec<Interval>>,
}
impl Coverage {
    pub(super) fn from_tile(tile: &LodTile) -> Self {
        let mut occupied: Vec<Vec<Interval>> = tile
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
            .collect();
        for tree in &tile.trees {
            if let (Some((min, max)), Some([x, z, mx, mz])) =
                (tree.bounds(), tree.column_bounds(tile.key))
            {
                for zz in z..mz {
                    for xx in x..mx {
                        occupied[xx + 32 * zz].push(Interval {
                            bottom: min[1],
                            top: max[1],
                        });
                    }
                }
            }
        }
        Self {
            intervals: tile.columns.iter().map(|c| c.coverage.clone()).collect(),
            occupied,
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

#[cfg(test)]
mod forest_tests {
    use super::*;
    #[test]
    fn forest_features_alone_hold_parent_until_all_crown_coverage_is_known() {
        use crate::lod::{Column, TileKey, TreeFeature};
        let columns = vec![
            Column {
                coverage: vec![Interval {
                    bottom: -64,
                    top: 128
                }],
                spans: vec![]
            };
            1024
        ];
        let key = TileKey {
            level: 1,
            x: -1,
            z: -1,
        };
        let parent = LodTile {
            key,
            revision: 1,
            columns: columns.clone(),
            geometric_error: 16,
            trees: vec![TreeFeature {
                anchor: [-16, 12, -16],
                trunk_height: 9,
                support_y: Some(13),
                log: crate::world::WOOD,
                leaves: crate::world::LEAVES,
                branch_x: crate::world::WOOD,
                branch_z: crate::world::WOOD,
                species: 0,
                shape: 0,
            }],
        };
        let parent = Coverage::from_tile(&parent);
        let mut children = key.children().unwrap().map(|key| {
            Coverage::from_tile(&LodTile {
                key,
                revision: 1,
                columns: columns.clone(),
                geometric_error: 8,
                trees: vec![],
            })
        });
        assert!(can_refine(
            &parent,
            [&children[0], &children[1], &children[2], &children[3]]
        ));
        for known in &mut children[3].intervals {
            known.clear();
        }
        assert!(
            !can_refine(
                &parent,
                [&children[0], &children[1], &children[2], &children[3]]
            ),
            "treecrown cannotvanish whenchildcoverageunknown"
        );
    }
}

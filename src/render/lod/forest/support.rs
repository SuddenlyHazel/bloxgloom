//! Connect source roots to certified exterior terrain in the coarse presentation.
use crate::{
    content::Catalog,
    lod::{LodTile, TreeFeature},
};

pub(in crate::render::lod) fn root_base(
    tile: &LodTile,
    tree: &TreeFeature,
    catalog: &Catalog,
) -> Option<i32> {
    let base = tree.support_y?;
    let [ox, oz, _, _] = tile.key.bounds()?;
    let width = tile.key.sample_width()?;
    let x = (i64::from(tree.anchor[0]) - i64::from(ox)).div_euclid(i64::from(width));
    let z = (i64::from(tree.anchor[2]) - i64::from(oz)).div_euclid(i64::from(width));
    if (0..32).contains(&x)
        && (0..32).contains(&z)
        && tree.support_in_column(&tile.columns[(x + 32 * z) as usize], catalog) != Some(base)
    {
        return None;
    }
    Some(i32::from(base))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lod::{Column, Interval, Span, TileKey},
        world::{LEAVES, STONE, WATER, WOOD},
    };
    #[test]
    fn roots_connect_only_to_certified_exterior_ground_preserving_caves_water_and_unknowns() {
        let catalog = crate::content::catalog();
        let tree = TreeFeature {
            anchor: [8, 12, 8],
            trunk_height: 9,
            support_y: Some(10),
            log: WOOD,
            leaves: LEAVES,
            branch_x: WOOD,
            branch_z: WOOD,
            species: 0,
            shape: 0,
        };
        let span = |bottom, top, state| Span {
            bottom,
            top,
            state,
            sky: 15,
            glow: 0,
        };
        let mut tile = LodTile {
            key: TileKey {
                level: 3,
                x: 0,
                z: 0,
            },
            revision: 1,
            geometric_error: 192,
            trees: vec![tree],
            columns: vec![
                Column {
                    coverage: vec![Interval {
                        bottom: -64,
                        top: 128
                    }],
                    spans: vec![span(-64, 10, STONE)]
                };
                1024
            ],
        };
        assert_eq!(root_base(&tile, &tree, catalog), Some(10));
        assert_eq!(
            tile.trees[0].anchor,
            [8, 12, 8],
            "source root and crown identity stay exact"
        );
        let mesh = crate::render::lod::mesh(
            &tile,
            &[],
            catalog,
            &crate::render::lod::FaceColors::new(catalog),
        )
        .unwrap();
        let mut child = crate::render::lod::coverage::Coverage::from_tile(&tile);
        for index in [66, 67, 98, 99] {
            child.intervals[index] = vec![
                Interval {
                    bottom: -64,
                    top: 10,
                },
                Interval {
                    bottom: 13,
                    top: 128,
                },
            ];
        }
        assert!(
            !crate::render::lod::coverage::can_refine(&mesh.coverage, [&child; 4]),
            "unknown finer gap must retain the connected coarse stem"
        );
        let mut old_coverage = mesh.coverage.as_ref().clone();
        old_coverage.occupied[33].retain(|i| !(i.bottom == 10 && i.top == 13));
        assert!(
            crate::render::lod::coverage::can_refine(&old_coverage, [&child; 4]),
            "fixture independently isolates the added stem coverage"
        );
        let column = &mut tile.columns[33];
        column.spans.push(span(12, 16, STONE));
        assert_eq!(
            root_base(&tile, &tree, catalog),
            None,
            "a cave roof forbids connection to its floor"
        );
        tile.columns[33].spans = vec![span(-64, 10, STONE), span(10, 12, WATER)];
        assert_eq!(
            root_base(&tile, &tree, catalog),
            None,
            "sampled water cannot create a submerged stem"
        );
        tile.columns[33].spans = vec![span(-64, 10, STONE)];
        tile.columns[33].coverage = vec![
            Interval {
                bottom: -64,
                top: 10,
            },
            Interval {
                bottom: 13,
                top: 128,
            },
        ];
        assert_eq!(
            root_base(&tile, &tree, catalog),
            None,
            "unknown gap cannot be bridged"
        );
        tile.columns[33].coverage = vec![Interval {
            bottom: -64,
            top: 128,
        }];
        tile.columns[33].spans = vec![span(-64, 9, STONE)];
        assert_eq!(
            tree.support_in_column(&tile.columns[33], catalog),
            None,
            "four metre cliff cannot invent a taller stem"
        );
        assert_eq!(root_base(&tile, &tree, catalog), None);
        let mut invalid = tree;
        invalid.support_y = Some(9);
        assert!(
            invalid.validate(tile.key, catalog).is_err(),
            "wire support bound is enforced independently"
        );
        tile.trees[0].support_y = None;
        let omitted = crate::render::lod::mesh(
            &tile,
            &[],
            catalog,
            &crate::render::lod::FaceColors::new(catalog),
        )
        .unwrap();
        assert!(omitted.vertices.iter().all(|v| v.position[1] <= 10.0));
        assert!(
            omitted.coverage.occupied[33]
                .iter()
                .any(|i| i.bottom == 13 && i.top == 24),
            "omitted proxy keeps canonical tree retirement coverage"
        );
        tile.columns[33].coverage.clear();
        assert_eq!(root_base(&tile, &tree, catalog), None);
    }
}

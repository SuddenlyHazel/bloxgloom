use super::*;
use crate::lod::Interval;
use crate::world::{DIRT, GRAVEL, SNOW, STONE, WATER};

fn span(bottom: i32, top: i32, state: crate::world::BlockId) -> Span {
    Span {
        bottom,
        top,
        state,
        sky: 15,
        glow: 0,
    }
}

#[test]
fn snow_is_a_source_thickness_cap_above_a_weighted_rock_body() {
    let catalog = Catalog::builtins();
    let mut columns = [Column {
        coverage: vec![Interval {
            bottom: -64,
            top: 128,
        }],
        spans: vec![span(-64, 14, STONE), span(14, 16, DIRT), span(16, 17, SNOW)],
    }];
    columns[0].spans[0].sky = 0;
    columns[0].spans[1].sky = 0;
    let coverage = columns[0].coverage.clone();
    condense(&mut columns, &catalog);
    assert_eq!(
        columns[0].spans,
        vec![
            Span {
                sky: 0,
                ..span(-64, 16, STONE)
            },
            span(16, 17, SNOW)
        ]
    );
    assert_eq!(columns[0].coverage, coverage);
    // Every side below the actual snow surface belongs to the body material.
    assert_eq!(
        columns[0]
            .spans
            .iter()
            .find(|s| s.bottom <= 15 && s.top > 15)
            .unwrap()
            .state,
        STONE
    );
}

#[test]
fn caves_unknown_gaps_and_water_never_become_solid_or_snow() {
    let catalog = Catalog::builtins();
    let source = Column {
        coverage: vec![
            Interval {
                bottom: -64,
                top: 4,
            },
            Interval {
                bottom: 8,
                top: 128,
            },
        ],
        spans: vec![
            span(-64, 0, STONE),
            span(0, 1, DIRT),
            span(3, 4, STONE),
            span(8, 11, STONE),
            span(11, 12, DIRT),
            span(12, 13, SNOW),
            span(13, 17, WATER),
        ],
    };
    let mut columns = [source.clone()];
    condense(&mut columns, &catalog);
    assert_eq!(columns[0].coverage, source.coverage);
    for y in -64..128 {
        let before = source.spans.iter().find(|s| s.bottom <= y && y < s.top);
        let after = columns[0].spans.iter().find(|s| s.bottom <= y && y < s.top);
        assert_eq!(before.is_some(), after.is_some(), "occupancy at{y}");
        if before.is_some_and(|s| s.state == WATER || s.state == SNOW) {
            assert_eq!(before.unwrap().state, after.unwrap().state);
        }
    }
}

#[test]
fn body_material_ties_and_identical_caps_are_deterministic_and_compact() {
    let catalog = Catalog::builtins();
    let mut columns = [Column {
        coverage: vec![Interval {
            bottom: 0,
            top: 128,
        }],
        spans: vec![span(0, 1, GRAVEL), span(1, 2, STONE), span(2, 3, STONE)],
    }];
    condense(&mut columns, &catalog);
    assert_eq!(columns[0].spans, vec![span(0, 3, STONE)]);
    let once = columns.clone();
    condense(&mut columns, &catalog);
    assert_eq!(columns, once);
}

#[test]
fn only_the_highest_solid_component_keeps_a_cap_under_water() {
    let catalog = Catalog::builtins();
    let mut columns = [Column {
        coverage: vec![Interval {
            bottom: -64,
            top: 128,
        }],
        spans: vec![
            span(-64, -40, STONE),
            span(-40, -39, DIRT),
            span(-30, -20, STONE),
            span(-20, -19, DIRT),
            span(8, 16, STONE),
            span(16, 17, SNOW),
            span(17, 20, WATER),
        ],
    }];
    condense(&mut columns, &catalog);
    assert_eq!(
        columns[0].spans,
        vec![
            span(-64, -39, STONE),
            span(-30, -19, STONE),
            span(8, 16, STONE),
            span(16, 17, SNOW),
            span(17, 20, WATER)
        ]
    );
    assert_eq!(
        columns[0].spans.iter().filter(|s| s.state == SNOW).count(),
        1
    );
}

#[test]
fn touching_logs_leaves_and_water_keep_their_optical_classes() {
    let catalog = crate::content::catalog();
    let log = catalog
        .state_by_key("bloxgloom:cherry_log[axis=y]")
        .unwrap();
    let leaves = catalog.state_by_key("bloxgloom:cherry_leaves").unwrap();
    let mut columns = [Column {
        coverage: vec![Interval {
            bottom: 0,
            top: 128,
        }],
        spans: vec![
            span(0, 8, STONE),
            span(8, 10, log),
            span(10, 11, leaves),
            span(11, 13, leaves),
            span(13, 17, WATER),
        ],
    }];
    let before = columns[0].clone();
    condense(&mut columns, catalog);
    for y in 0..17 {
        let source = before
            .spans
            .iter()
            .find(|s| s.bottom <= y && y < s.top)
            .unwrap();
        let retained = columns[0]
            .spans
            .iter()
            .find(|s| s.bottom <= y && y < s.top)
            .unwrap();
        let flags = FLUID | CUTOUT | OPAQUE;
        assert_eq!(
            catalog.block_flags(retained.state) & flags,
            catalog.block_flags(source.state) & flags,
            "optical class at{y}"
        );
    }
    assert!(
        columns[0]
            .spans
            .iter()
            .any(|s| s.bottom == 10 && s.top == 13 && s.state == leaves)
    );
    assert!(
        columns[0]
            .spans
            .iter()
            .any(|s| s.bottom == 8 && s.top == 10 && s.state == log)
    );
    assert!(
        columns[0]
            .spans
            .iter()
            .any(|s| s.bottom == 13 && s.top == 17 && s.state == WATER)
    );
}

#[test]
fn source_surface_caps_survive_packet_reduction_and_coarse_parent_union() {
    use crate::lod::{LodTile, TILE_COLUMNS, TileKey};
    let catalog = Catalog::builtins();
    let parent = TileKey {
        level: 1,
        x: -1,
        z: -1,
    };
    let children = parent.children().unwrap().map(|key| {
        let tile = LodTile {
            key,
            revision: 1,
            geometric_error: 0,
            trees: vec![],
            columns: vec![
                Column {
                    coverage: vec![Interval {
                        bottom: -64,
                        top: 128
                    }],
                    spans: vec![
                        span(-64, -30, STONE),
                        span(-30, -29, DIRT),
                        span(-29, 14, STONE),
                        span(14, 16, DIRT),
                        span(16, 17, SNOW)
                    ],
                };
                TILE_COLUMNS
            ],
        };
        assert!(tile.validate(&catalog).is_err());
        let reduced = tile.into_render_summary(&catalog).unwrap();
        assert!(reduced.encoded_bytes() <= crate::lod::MAX_TILE_BYTES);
        for column in &reduced.columns {
            assert_eq!(column.spans, vec![span(-64, 16, STONE), span(16, 17, SNOW)]);
        }
        reduced
    });
    let reduced = crate::lod::reduce_parent(
        parent,
        2,
        [&children[0], &children[1], &children[2], &children[3]],
        &catalog,
    )
    .unwrap();
    assert!(
        reduced
            .columns
            .iter()
            .all(|c| c.spans == children[0].columns[0].spans)
    );
}

#[test]
#[ignore = "CPU-only production77tile surface-cap packet/geometry probe; run --ignored --nocapture"]
fn actual_coast_and_origin_surface_cap_packet_probe() {
    use crate::render::lod::{FaceColors, desired_tiles, mesh};
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let mut failures = Vec::new();
    for (name, camera) in [
        ("coast", glam::Vec3::new(-617.5, 37.0, -2015.5)),
        ("perf-origin", glam::Vec3::new(0.5, 50.0, -24.0)),
    ] {
        let start = std::time::Instant::now();
        let keys = desired_tiles(camera, 512, 1, 4);
        let mut tiles = Vec::new();
        let mut errors = Vec::new();
        for key in keys.iter().copied() {
            match crate::world::lod::builtin_lod_tile(key, 1, 0xB10C_6100, catalog) {
                Ok(tile) => tiles.push(tile),
                Err(error) => errors.push(format!("{key:?}: {error}")),
            }
        }
        let summary_ms = start.elapsed().as_secs_f64() * 1000.0;
        let bytes: usize = tiles.iter().map(crate::lod::LodTile::encoded_bytes).sum();
        let largest = tiles
            .iter()
            .map(|tile| (tile.encoded_bytes(), tile.key))
            .max()
            .unwrap();
        let started = std::time::Instant::now();
        let mut mesh_bytes = 0;
        let mut triangles = 0;
        for tile in &tiles {
            let a = tile.key.bounds().unwrap();
            let neighbors = tiles
                .iter()
                .filter(|other| {
                    let b = other.key.bounds().unwrap();
                    ((a[2] == b[0] || b[2] == a[0]) && a[1] < b[3] && b[1] < a[3])
                        || ((a[3] == b[1] || b[3] == a[1]) && a[0] < b[2] && b[0] < a[2])
                })
                .collect::<Vec<_>>();
            let m = mesh(tile, &neighbors, catalog, &colors).unwrap();
            mesh_bytes += m.byte_len();
            triangles += (m.indices.len() + m.water_indices.len()) / 3;
        }
        eprintln!(
            "surface caps {name}: {}/{} admitted summary={summary_ms:.2}ms bytes={bytes} largest={largest:?} mesh={:.2}ms mesh_bytes={mesh_bytes} triangles={triangles}; failures={errors:?}",
            tiles.len(),
            keys.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        failures.extend(errors.into_iter().map(|error| format!("{name}: {error}")));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

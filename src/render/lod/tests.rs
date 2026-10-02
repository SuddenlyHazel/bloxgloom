use super::*;
use crate::{
    lod::{Column, Interval, LodTile, Span, TILE_COLUMNS, TileKey},
    world::STONE,
};
use std::collections::HashSet;
fn fixture(key: TileKey) -> LodTile {
    LodTile {
        key,
        revision: 1,
        columns: vec![Column::default(); TILE_COLUMNS],
        geometric_error: 0,
    }
}
fn column(spans: &[(i32, i32)]) -> Column {
    Column {
        coverage: vec![Interval {
            bottom: -100,
            top: 100,
        }],
        spans: spans
            .iter()
            .map(|&(bottom, top)| Span {
                bottom,
                top,
                state: STONE,
                sky: 0,
                glow: 0,
            })
            .collect(),
    }
}
#[test]
fn parent_stays_until_four_uploaded_children_replace_it() {
    let parent = TileKey {
        level: 1,
        x: -1,
        z: 0,
    };
    let children = parent.children().unwrap();
    let mut ready = HashSet::from([parent]);
    for c in children.iter().take(3) {
        ready.insert(*c);
        assert_eq!(
            super::gpu::select_ready(ready.clone(), |_, _| true),
            vec![parent]
        );
    }
    ready.insert(children[3]);
    let selected = super::gpu::select_ready(ready, |_, _| true);
    assert_eq!(selected.len(), 4);
    assert!(!selected.contains(&parent));
    for c in children {
        assert!(selected.contains(&c));
    }
}
#[test]
fn bridge_keeps_both_gap_faces_and_sides() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let mut tile = fixture(TileKey {
        level: 0,
        x: -1,
        z: 0,
    });
    tile.columns[0] = column(&[(0, 4), (10, 11)]);
    let mesh = mesh(&tile, &[], catalog, &colors).unwrap();
    assert_eq!(mesh.indices.len() / 6, 12);
    let heights: Vec<_> = mesh
        .vertices
        .chunks_exact(11)
        .filter(|v| v[4] != 0.0)
        .map(|v| v[1] as i32)
        .collect();
    for y in [0, 4, 10, 11] {
        assert!(heights.contains(&y));
    }
    assert!(mesh.vertices.chunks_exact(11).all(|v| v[9] == 0.0));
    // Local coordinates retain small magnitudes even for negative world origins.
    assert!(
        mesh.vertices
            .chunks_exact(11)
            .all(|v| v[0] >= 0.0 && v[0] <= 1.0)
    );
}
#[test]
fn coarse_boundary_splits_only_where_fine_spans_differ() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let mut tile = fixture(TileKey {
        level: 1,
        x: 0,
        z: 0,
    });
    tile.columns[31] = column(&[(0, 10)]);
    let mut neighbor = fixture(TileKey {
        level: 0,
        x: 2,
        z: 0,
    });
    neighbor.columns[0] = column(&[(0, 4)]);
    neighbor.columns[32] = column(&[(0, 8)]);
    let mesh = mesh(&tile, &[&neighbor], catalog, &colors).unwrap();
    let side: Vec<_> = mesh
        .vertices
        .chunks_exact(11)
        .filter(|v| v[3] == 1.0)
        .collect();
    assert!(side.len() >= 8);
    assert!(side.iter().any(|v| v[1] == 4.0));
    assert!(side.iter().any(|v| v[1] == 8.0));
}
#[test]
fn distant_shader_validates_and_uses_three_dimensional_coverage() {
    let source = super::super::daylight::surface_shader(include_str!("shader.wgsl"));
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn request_rings_are_bounded_and_coarse_first() {
    for horizon in [512, 1024] {
        for quality in 0..=2 {
            let tiles = desired_tiles(glam::Vec3::new(-0.1, 700.0, -512.1), horizon, quality, 4);
            assert!(tiles.len() < 128);
            assert!(tiles.windows(2).all(|p| p[0].level >= p[1].level));
            assert!(tiles.iter().all(|k| k.bounds().is_some()));
        }
    }
    assert!(desired_tiles(glam::Vec3::ZERO, 0, 2, 4).is_empty());
}

#[test]
fn flat_roof_caps_merge_without_filling_openings() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let mut tile = fixture(TileKey {
        level: 3,
        x: 0,
        z: 0,
    });
    tile.columns.fill(column(&[(0, 8)]));
    let flat = mesh(&tile, &[], catalog, &colors).unwrap();
    assert_eq!(
        flat.vertices
            .chunks_exact(11)
            .filter(|v| v[4] != 0.0)
            .count(),
        8
    );
    assert!(flat.byte_len() < 32 * 1024);
    tile.columns[16 + 32 * 16] = Column::default();
    let opening = mesh(&tile, &[], catalog, &colors).unwrap();
    let caps: Vec<_> = opening
        .vertices
        .chunks_exact(44)
        .filter(|v| v[4] != 0.0)
        .collect();
    for quad in caps {
        let minx = quad
            .chunks_exact(11)
            .map(|v| v[0])
            .fold(f32::INFINITY, f32::min);
        let maxx = quad
            .chunks_exact(11)
            .map(|v| v[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let minz = quad
            .chunks_exact(11)
            .map(|v| v[2])
            .fold(f32::INFINITY, f32::min);
        let maxz = quad
            .chunks_exact(11)
            .map(|v| v[2])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(!(minx < 136.0 && maxx > 128.0 && minz < 136.0 && maxz > 128.0));
    }
}

#[test]
fn sky_lit_roof_does_not_light_its_retained_cavity() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let mut tile = fixture(TileKey {
        level: 0,
        x: 0,
        z: 0,
    });
    tile.columns[0] = column(&[(0, 12)]);
    tile.columns[0].spans[0].sky = 15;
    tile.columns[1] = column(&[(0, 4), (10, 12)]);
    tile.columns[1].spans[1].sky = 15;
    let mesh = mesh(&tile, &[], catalog, &colors).unwrap();
    let wall: Vec<_> = mesh
        .vertices
        .chunks_exact(11)
        .filter(|v| v[0] == 1.0 && v[3] == 1.0)
        .collect();
    assert!(!wall.is_empty());
    assert!(wall.iter().all(|v| v[9] == 0.0));
    let ceiling: Vec<_> = mesh
        .vertices
        .chunks_exact(11)
        .filter(|v| v[1] == 10.0 && v[4] == -1.0)
        .collect();
    assert!(!ceiling.is_empty());
    assert!(ceiling.iter().all(|v| v[9] == 0.0));
}

#[test]
fn unknown_child_height_cannot_remove_parent_bridge() {
    use super::coverage::{Coverage, can_refine};
    let key = TileKey {
        level: 1,
        x: 0,
        z: 0,
    };
    let mut parent = fixture(key);
    parent.columns[0] = column(&[(40, 42)]);
    let p = Coverage::from_tile(&parent);
    let mut children = key.children().unwrap().map(fixture);
    for c in &mut children {
        c.columns.fill(column(&[]));
    }
    let good = children.each_ref().map(Coverage::from_tile);
    assert!(can_refine(&p, good.each_ref()));
    children[0].columns[1].coverage = vec![Interval { bottom: 0, top: 16 }];
    let partial = children.each_ref().map(Coverage::from_tile);
    assert!(!can_refine(&p, partial.each_ref()));
    let ready = std::iter::once(key)
        .chain(key.children().unwrap())
        .collect();
    assert_eq!(
        super::gpu::select_ready(ready, |_, _| can_refine(&p, partial.each_ref())),
        vec![key]
    );
}

#[test]
fn refinement_requests_complete_sibling_families_at_negative_boundaries() {
    for position in [
        glam::Vec3::ZERO,
        glam::Vec3::new(-0.1, 80.0, -512.1),
        glam::Vec3::new(511.9, 0.0, 32.1),
    ] {
        for max_level in [3, 4] {
            for horizon in [512, 1024] {
                for quality in 0..=2 {
                    let keys = desired_tiles(position, horizon, quality, max_level);
                    assert!(keys.len() <= 128);
                    let set: HashSet<_> = keys.iter().copied().collect();
                    for key in keys.iter().filter(|k| k.level < max_level) {
                        for sibling in key.parent().unwrap().children().unwrap() {
                            assert!(set.contains(&sibling));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn coordinate_and_vertical_extremes_mesh_without_wrapping() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let mut tile = fixture(TileKey {
        level: 0,
        x: i32::MIN / 32,
        z: i32::MIN / 32,
    });
    tile.columns[0] = Column {
        coverage: vec![Interval {
            bottom: i32::MIN,
            top: i32::MAX,
        }],
        spans: vec![Span {
            bottom: i32::MIN,
            top: i32::MAX,
            state: STONE,
            sky: 0,
            glow: 0,
        }],
    };
    assert!(mesh(&tile, &[], catalog, &colors).is_err());
    tile.columns[0].spans[0].top = i32::MIN + 1;
    tile.columns[0].spans.push(Span {
        bottom: i32::MAX - 1,
        top: i32::MAX,
        state: STONE,
        sky: 0,
        glow: 0,
    });
    let mesh = mesh(&tile, &[], catalog, &colors).unwrap();
    assert_eq!(mesh.indices.len(), 72);
    assert!(mesh.vertices.iter().all(|value| value.is_finite()));
    assert!(mesh.vertices.chunks_exact(11).any(|v| v[1] > 0.0));
    assert!(mesh.vertices.chunks_exact(11).any(|v| v[1] < 0.0));
}

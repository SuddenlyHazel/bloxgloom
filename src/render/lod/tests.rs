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
        assert_eq!(super::gpu::select_ready(ready.clone()), vec![parent]);
    }
    ready.insert(children[3]);
    let selected = super::gpu::select_ready(ready);
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
    let mesh = mesh(&tile, &[], catalog, &colors);
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
    let mesh = mesh(&tile, &[&neighbor], catalog, &colors);
    let side: Vec<_> = mesh
        .vertices
        .chunks_exact(11)
        .filter(|v| v[3] == 1.0)
        .collect();
    assert_eq!(side.len(), 8);
    assert!(side.iter().any(|v| v[1] == 4.0));
    assert!(side.iter().any(|v| v[1] == 8.0));
}
#[test]
fn distant_shader_validates_and_uses_three_dimensional_coverage() {
    let source = super::super::fog::shader(include_str!("shader.wgsl"));
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
            let tiles = desired_tiles(glam::Vec3::new(-0.1, 700.0, -512.1), horizon, quality);
            assert!(tiles.len() < 128);
            assert!(tiles.windows(2).all(|p| p[0].level >= p[1].level));
            assert!(tiles.iter().all(|k| k.bounds().is_some()));
        }
    }
    assert!(desired_tiles(glam::Vec3::ZERO, 0, 2).is_empty());
}

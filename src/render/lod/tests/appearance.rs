use super::*;
use crate::world::{GRAVEL, WATER};
#[test]
fn transparent_water_retains_opaque_bed_and_removes_internal_fluid_faces() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let mut tile = fixture(TileKey {
        level: 1,
        x: 0,
        z: 0,
    });
    for x in [0, 1] {
        tile.columns[x] = Column {
            coverage: vec![Interval { bottom: 0, top: 16 }],
            spans: vec![
                Span {
                    bottom: 0,
                    top: 4,
                    state: GRAVEL,
                    sky: 12,
                    glow: 0,
                },
                Span {
                    bottom: 4,
                    top: 8,
                    state: WATER,
                    sky: 15,
                    glow: 0,
                },
            ],
        };
    }
    let mesh = mesh(&tile, &[], catalog, &colors).unwrap();
    let vertices = |indices: &[u32]| {
        indices
            .iter()
            .map(|i| mesh.vertices[*i as usize].unpack())
            .collect::<Vec<_>>()
    };
    let solid = vertices(&mesh.indices);
    let fluid = vertices(&mesh.water_indices);
    assert!(
        solid.iter().any(|v| v[1] == 4.0 && v[4] == 1.0),
        "opaque bed top survives beneath translucent water"
    );
    assert!(fluid.iter().any(|v| v[1] == 8.0 && v[4] == 1.0));
    assert!(
        fluid.iter().all(|v| !(v[1] == 4.0 && v[4] == -1.0)),
        "no hidden fluid bottom against bed"
    );
    assert!(
        fluid.iter().all(|v| !(v[0] == 2.0 && v[3] != 0.0)),
        "no fluid plane within adjacent wet columns"
    );
    assert!(
        mesh.bounds.unwrap()[1].y == 8.0,
        "fluid participates in visibility bounds"
    );
}
#[test]
fn fluid_only_tiles_remain_drawable_and_count_toward_geometry_budget() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let mut tile = fixture(TileKey {
        level: 0,
        x: -1,
        z: -1,
    });
    tile.columns[0] = Column {
        coverage: vec![Interval { bottom: 0, top: 16 }],
        spans: vec![Span {
            bottom: 2,
            top: 3,
            state: WATER,
            sky: 15,
            glow: 0,
        }],
    };
    let mesh = mesh(&tile, &[], catalog, &colors).unwrap();
    assert!(mesh.indices.is_empty());
    assert_eq!(mesh.water_indices.len(), 36);
    assert_eq!(mesh.byte_len(), 24 * 20 + 36 * 4);
    assert!(mesh.bounds.is_some());
}
#[test]
fn only_transition_levels_receive_actual_face_textures() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    for level in 0..=4 {
        let surface = super::super::surface::Surface::new(catalog, &colors, STONE, 1, 1, level);
        assert_eq!(surface.layer.is_some(), level <= 2);
        let water = super::super::surface::Surface::new(catalog, &colors, WATER, 1, 1, level);
        assert!(water.fluid && water.layer.is_none() && water.color[3] < 1.0);
    }
}

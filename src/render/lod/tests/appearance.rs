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
        assert_eq!(surface.sample_texture, level <= 2);
        assert_eq!(
            surface.layer.is_some(),
            level <= 2 || crate::render::bsl_reference::enabled()
        );
        let water = super::super::surface::Surface::new(catalog, &colors, WATER, 1, 1, level);
        assert!(water.fluid && water.layer.is_none() && water.color[3] < 1.0);
    }
}

#[test]
fn generated_ocean_peers_share_source_height_and_have_no_surface_sidewall() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let a = crate::world::lod::builtin_lod_tile(
        TileKey {
            level: 2,
            x: -6,
            z: -18,
        },
        1,
        0xB10C_6100,
        catalog,
    )
    .unwrap();
    let b = crate::world::lod::builtin_lod_tile(
        TileKey {
            level: 2,
            x: -5,
            z: -18,
        },
        1,
        0xB10C_6100,
        catalog,
    )
    .unwrap();
    let mesh = mesh(&a, &[&b], catalog, &colors).unwrap();
    let wet = |c: &Column| {
        c.spans
            .iter()
            .any(|s| s.state == WATER && s.top == 17 && s.bottom <= 16)
    };
    let rows: Vec<_> = (0..32)
        .filter(|z| wet(&a.columns[31 + 32 * z]) && wet(&b.columns[32 * z]))
        .collect();
    assert!(rows.len() >= 8, "real ocean peer seam fixture");
    let mut caps = 0;
    for triangle in mesh.water_indices.chunks_exact(3) {
        let points: Vec<_> = triangle
            .iter()
            .map(|i| mesh.vertices[*i as usize].unpack())
            .collect();
        if points.iter().all(|p| p[4] > 0.5 && p[1] == 17.0) {
            caps += 1;
        }
        if points.iter().all(|p| p[3] > 0.5 && p[0] == 128.0) {
            let z = (points.iter().map(|p| p[2]).sum::<f32>() / 3.0 / 4.0).floor() as usize;
            if rows.contains(&z) {
                assert!(
                    points.iter().all(|p| p[1] <= 16.0),
                    "known ocean peers cannot manufacture a surface sidewall"
                );
            }
        }
    }
    assert!(caps > 0);
}

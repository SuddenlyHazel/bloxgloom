use super::*;
use crate::{
    lod::{Interval, Span, TileKey},
    world::{SNOW, STONE, WATER},
};

fn ramp(key: TileKey, layered: bool) -> LodTile {
    let [ox, _, _, _] = key.bounds().unwrap();
    let w = key.sample_width().unwrap();
    LodTile {
        key,
        revision: 1,
        trees: vec![],
        geometric_error: 4,
        columns: (0..1024)
            .map(|i| {
                let top = 20 + (ox / w) + i % 32;
                Column {
                    coverage: vec![Interval {
                        bottom: BEDROCK_Y,
                        top: 128,
                    }],
                    spans: if layered {
                        vec![
                            Span {
                                bottom: BEDROCK_Y,
                                top: top - 1,
                                state: STONE,
                                sky: 0,
                                glow: 0,
                            },
                            Span {
                                bottom: top - 1,
                                top,
                                state: SNOW,
                                sky: 15,
                                glow: 0,
                            },
                        ]
                    } else {
                        vec![Span {
                            bottom: BEDROCK_Y,
                            top,
                            state: STONE,
                            sky: 15,
                            glow: 0,
                        }]
                    },
                }
            })
            .collect(),
    }
}
#[test]
fn gentle_caps_share_edges_and_form_real_nonzero_triangles() {
    let catalog = crate::content::catalog();
    let colors = super::super::FaceColors::new(catalog);
    for layered in [false, true] {
        let tile = ramp(
            TileKey {
                level: 2,
                x: 0,
                z: 0,
            },
            layered,
        );
        let m = super::super::mesh(&tile, &[], catalog, &colors).unwrap();
        let shared: Vec<_> = m
            .vertices
            .iter()
            .map(|v| v.unpack())
            .filter(|p| p[0] == 8.0 && p[2] == 8.0 && p[4] > 0.5)
            .collect();
        assert!(shared.len() >= 2, "both adjacent cap corners must exist");
        assert!(
            shared.iter().all(|p| p[1] == 21.5),
            "all cap corners must weld to a shared height: {shared:?}"
        );
        for t in m.indices.chunks_exact(3) {
            let p = [t[0], t[1], t[2]].map(|i| glam::Vec3::from(m.vertices[i as usize].position));
            assert!((p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 0.0);
        }
        assert!(
            m.bounds.unwrap()[0].y == BEDROCK_Y as f32,
            "buried ground cannot be moved"
        );
        let ray = super::super::ray::extract(&m).unwrap();
        assert!(
            ray.triangles
                .iter()
                .any(|t| t.normal[1] > 0.5 && t.normal[0].abs() > 0.01),
            "reconstructed cap ray normals must follow the actual slope"
        );
        assert!(
            ray.triangles.iter().all(|t| t.surface_color >> 24 == 255),
            "the packed reconstruction marker cannot alter opaque alpha"
        );
    }
}
#[test]
fn matching_tile_peers_share_caps_and_unknown_or_wet_boundaries_keep_voxel_heights() {
    let catalog = crate::content::catalog();
    let colors = super::super::FaceColors::new(catalog);
    let a = ramp(
        TileKey {
            level: 2,
            x: 0,
            z: 0,
        },
        true,
    );
    let b = ramp(
        TileKey {
            level: 2,
            x: 1,
            z: 0,
        },
        true,
    );
    let ma = super::super::mesh(&a, &[&b], catalog, &colors).unwrap();
    let mb = super::super::mesh(&b, &[&a], catalog, &colors).unwrap();
    let seam = |m: &super::super::Mesh, x: f32| {
        m.vertices
            .iter()
            .map(|v| v.unpack())
            .filter(|p| p[0] == x && p[2] == 8.0 && p[4] > 0.5)
            .map(|p| p[1])
            .collect::<Vec<_>>()
    };
    assert!(seam(&ma, 128.0).iter().all(|h| *h == 51.5));
    assert!(seam(&mb, 0.0).iter().all(|h| *h == 51.5));
    let alone = super::super::mesh(&a, &[], catalog, &colors).unwrap();
    assert!(seam(&alone, 128.0).iter().all(|h| *h == 51.0));
    let mut wet = b.clone();
    for c in &mut wet.columns {
        c.spans.push(Span {
            bottom: c.spans.last().unwrap().top,
            top: 100,
            state: WATER,
            sky: 15,
            glow: 0,
        });
    }
    let dryedge = super::super::mesh(&a, &[&wet], catalog, &colors).unwrap();
    assert!(seam(&dryedge, 128.0).iter().all(|h| *h == 51.0));
}
#[test]
fn caves_roofs_artificial_blocks_and_steep_cliffs_are_not_reconstructed() {
    let catalog = crate::content::catalog();
    let mut t = ramp(
        TileKey {
            level: 2,
            x: 0,
            z: 0,
        },
        false,
    );
    t.columns[8 + 32 * 8].spans[0].bottom = 26;
    assert!(
        Shape::new(&t, &[], catalog, true).top(8, 8).is_none(),
        "a thin retained roof is not a heightfield"
    );
    t.columns[8 + 32 * 8].spans[0].bottom = BEDROCK_Y;
    t.columns[8 + 32 * 8].spans[0].top += 20;
    let s = Shape::new(&t, &[], catalog, true);
    assert!(!s.cap_changes(8, 8, 48), "steep cliffs stay exact");
    t.columns[8 + 32 * 8].spans[0].state = crate::world::WOOD;
    assert!(Shape::new(&t, &[], catalog, true).top(8, 8).is_none());
}

#[test]
fn deep_cave_surfaces_stay_exact_while_the_thick_ground_shell_is_reconstructed() {
    let catalog = crate::content::catalog();
    let colors = super::super::FaceColors::new(catalog);
    let mut tile = ramp(
        TileKey {
            level: 2,
            x: 0,
            z: 0,
        },
        true,
    );
    for c in &mut tile.columns {
        c.spans[0].bottom = 5;
        c.spans.insert(
            0,
            Span {
                bottom: BEDROCK_Y,
                top: 2,
                state: STONE,
                sky: 0,
                glow: 0,
            },
        );
    }
    let original =
        super::super::mesh::mesh_with_surface_shape(&tile, &[], catalog, &colors, false).unwrap();
    let shaped =
        super::super::mesh::mesh_with_surface_shape(&tile, &[], catalog, &colors, true).unwrap();
    let buried = |mesh: &super::super::Mesh| {
        let mut vertices: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|v| v.position[1] <= 5.0)
            .map(|v| bytemuck::bytes_of(v).to_vec())
            .collect();
        vertices.sort();
        vertices
    };
    assert_eq!(
        buried(&original),
        buried(&shaped),
        "the deep floor, roof and sidewall vertices must remain bit-identical"
    );
    assert!(
        shaped
            .vertices
            .iter()
            .any(|v| v.ray_surface().reconstructed)
    );
    let corner: Vec<_> = shaped
        .vertices
        .iter()
        .map(|v| v.unpack())
        .filter(|p| p[0] == 8.0 && p[2] == 8.0 && p[4] > 0.5 && p[1] > 5.0)
        .collect();
    assert!(
        corner.iter().all(|p| p[1] == 21.5),
        "surface caps above the cave must share their reconstructed height"
    );
}

#[test]
fn shared_corner_above_a_nearby_cave_mouth_cannot_move_its_roof_boundary() {
    let catalog = crate::content::catalog();
    let mut tile = ramp(
        TileKey {
            level: 2,
            x: 0,
            z: 0,
        },
        false,
    );
    // Both roofs satisfy the individual four-metre shell rule. The higher
    // cavity boundary is still inside the lower roof's deformation band.
    tile.columns[2 + 32 * 2].spans[0].bottom = 18;
    tile.columns[1 + 32 * 2].spans[0].bottom = 17;
    let shape = Shape::new(&tile, &[], catalog, true);
    assert!(shape.top(2, 2).is_some());
    assert!(
        shape.height(8.0, 8.0).is_none(),
        "shared corners must reject an adjacent cavity boundary inside any roof's deformation band"
    );
}

#[test]
fn retained_tree_roots_keep_the_original_ground_shell() {
    let catalog = crate::content::catalog();
    let mut t = ramp(
        TileKey {
            level: 2,
            x: 0,
            z: 0,
        },
        false,
    );
    t.trees.push(crate::lod::TreeFeature {
        anchor: [9, 22, 9],
        trunk_height: 6,
        support_y: Some(23),
        log: crate::world::WOOD,
        leaves: crate::world::LEAVES,
        branch_x: crate::world::WOOD,
        branch_z: crate::world::WOOD,
        species: 0,
        shape: 0,
    });
    let s = Shape::new(&t, &[], catalog, true);
    assert!(s.top(2, 2).is_none());
    assert!(!s.cap_changes(2, 2, 22));
    for (x, z) in [(8.0, 8.0), (12.0, 8.0), (8.0, 12.0), (12.0, 12.0)] {
        assert!(
            s.height(x, z).is_none(),
            "a tree support cell's shared corners must stay exact"
        );
    }
}

#[test]
#[ignore = "CPU-only natural coast mesh budget comparison; run explicitly"]
fn natural_coast_reconstruction_geometry_budget() {
    let catalog = crate::content::catalog();
    let colors = super::super::FaceColors::new(catalog);
    let keys = super::super::desired_tiles(glam::Vec3::new(-617.5, 37.0, -2015.5), 512, 1, 4);
    let tiles: Vec<_> = keys
        .into_iter()
        .map(|key| crate::world::lod::builtin_lod_tile(key, 1, 0xB10C_6100, catalog).unwrap())
        .collect();
    let mut totals = [[0usize; 3]; 2];
    for tile in &tiles {
        let a = tile.key.bounds().unwrap();
        let neighbors: Vec<_> = tiles
            .iter()
            .filter(|n| {
                let b = n.key.bounds().unwrap();
                ((a[2] == b[0] || b[2] == a[0]) && a[1] < b[3] && b[1] < a[3])
                    || ((a[3] == b[1] || b[3] == a[1]) && a[0] < b[2] && b[0] < a[2])
            })
            .collect();
        for (i, reconstruct) in [false, true].into_iter().enumerate() {
            let m = super::super::mesh::mesh_with_surface_shape(
                tile,
                &neighbors,
                catalog,
                &colors,
                reconstruct,
            )
            .unwrap();
            totals[i][0] += m.byte_len();
            totals[i][1] += m.indices.len() / 3;
            totals[i][2] += m.water_indices.len() / 3;
        }
    }
    eprintln!(
        "coast tiles={} before(bytes,opaque_tri,water_tri)={:?} after={:?}",
        tiles.len(),
        totals[0],
        totals[1]
    );
    assert!(totals[1][0] < 128 * 1024 * 1024);
}

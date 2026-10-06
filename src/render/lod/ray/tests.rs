use super::*;
use crate::lod::TileKey;
use crate::render::lod::{coverage::Coverage, surface::Surface};

fn empty() -> Mesh {
    Mesh {
        key: TileKey {
            level: 1,
            x: -1,
            z: -2,
        },
        revision: 7,
        loading: None,
        ray: None,
        vertices: vec![],
        indices: vec![],
        water_indices: vec![],
        bounds: None,
        coverage: Box::new(Coverage {
            intervals: vec![],
            occupied: vec![],
        }),
    }
}
fn append(mesh: &mut Mesh, axis: usize, side: i32, surface: Surface) {
    let start = mesh.vertices.len() as u32;
    for position in [[1.0, 2.0, 3.0], [1.0, 4.0, 3.0], [1.0, 4.0, 5.0]] {
        mesh.vertices
            .push(Vertex::new(position, axis, side, [0.2, 0.345, 0.8], 7, 3).material(surface));
    }
    let indices = if surface.fluid {
        &mut mesh.water_indices
    } else {
        &mut mesh.indices
    };
    indices.extend([start, start + 1, start + 2]);
}
fn surface() -> Surface {
    Surface {
        color: [0.2, 0.345, 0.8, 1.0],
        layer: Some(17),
        sample_texture: true,
        fluid: false,
        cutout: false,
    }
}

#[test]
fn negative_tile_origins_axis_uvs_and_quantized_appearance_match_raster_vertices() {
    for axis in 0..3 {
        for side in [-1, 1] {
            let mut mesh = empty();
            append(&mut mesh, axis, side, surface());
            let chunk = extract(&mesh).unwrap();
            assert!(
                chunk.key.is_none(),
                "coarse summaries cannot certify loaded air"
            );
            assert_eq!(chunk.triangles.len(), 1);
            let t = chunk.triangles[0];
            assert_eq!(t.a, [-63.0, 2.0, -125.0, 17.0]);
            assert_eq!(t.b[..3], [-63.0, 4.0, -125.0]);
            assert_eq!(t.c[..3], [-63.0, 4.0, -123.0]);
            let expected_uv = match axis {
                0 => [[3.0, -2.0], [3.0, -4.0], [5.0, -4.0]],
                1 => [[3.0, 1.0], [3.0, 1.0], [5.0, 1.0]],
                _ => [[1.0, -2.0], [1.0, -4.0], [1.0, -4.0]],
            };
            assert_eq!(
                t.uv_ab,
                [
                    expected_uv[0][0],
                    expected_uv[0][1],
                    expected_uv[1][0],
                    expected_uv[1][1]
                ]
            );
            assert_eq!(t.uv_c, expected_uv[2]);
            assert_eq!(t.normal[axis], side as f32);
            assert_eq!(t.normal[3], -1.0);
            assert_eq!(t.b[3], 7.0 / 15.0);
            assert_eq!(t.surface_color, u32::from_le_bytes([51, 88, 204, 255]));
            assert_eq!(
                t.surface_flags,
                surface::LOD
                    | surface::NO_WIND
                    | surface::TEXTURE
                    | surface::COARSE_COLOR
                    | (3 << surface::GLOW_SHIFT)
            );
        }
    }
}

#[test]
fn fluid_cutout_and_admitted_coarse_color_have_distinct_material_contracts() {
    let mut mesh = empty();
    append(
        &mut mesh,
        0,
        -1,
        Surface {
            cutout: true,
            ..surface()
        },
    );
    append(
        &mut mesh,
        1,
        1,
        Surface {
            sample_texture: false,
            ..surface()
        },
    );
    append(
        &mut mesh,
        2,
        1,
        Surface {
            layer: None,
            sample_texture: false,
            fluid: true,
            color: [0.2, 0.345, 0.8, 0.7],
            ..surface()
        },
    );
    let chunk = extract(&mesh).unwrap();
    assert_eq!(
        chunk.triangles.len(),
        (mesh.indices.len() + mesh.water_indices.len()) / 3
    );
    assert_eq!(chunk.triangles[0].c[3], 1.0);
    assert_eq!(
        chunk.triangles[1].a[3], 17.0,
        "coarse appearance retains valid material identity"
    );
    assert_eq!(
        chunk.triangles[1].surface_flags,
        surface::LOD | surface::NO_WIND | surface::COARSE_COLOR | (3 << surface::GLOW_SHIFT)
    );
    let water = chunk.triangles[2];
    assert_eq!(
        water.a[3], 0.0,
        "untextured water uses valid fallback identity"
    );
    assert_eq!(water.c[3], 0.0);
    assert_eq!(
        water.surface_flags,
        surface::LOD
            | surface::NO_WIND
            | surface::COARSE_COLOR
            | surface::WATER
            | (3 << surface::GLOW_SHIFT)
    );
    assert_eq!(water.surface_color >> 24, 179);
}

#[test]
fn source_forest_triangle_silhouettes_and_static_cutout_contract_are_preserved() {
    let catalog = crate::content::catalog();
    let colors = crate::render::lod::FaceColors::new(catalog);
    let mut mesh = empty();
    let tree = crate::lod::TreeFeature {
        anchor: [-48, 12, -112],
        trunk_height: 9,
        support_y: Some(13),
        log: crate::world::WOOD,
        leaves: crate::world::LEAVES,
        branch_x: crate::world::WOOD,
        branch_z: crate::world::WOOD,
        species: 0,
        shape: 0,
    };
    crate::render::lod::forest::append(&mut mesh, &tree, catalog, &colors).unwrap();
    let chunk = extract(&mesh).unwrap();
    assert!(!chunk.triangles.is_empty());
    assert_eq!(chunk.triangles.len(), mesh.indices.len() / 3);
    let mut cutouts = 0;
    for (triangle, face) in chunk.triangles.iter().zip(mesh.indices.chunks_exact(3)) {
        let positions = [triangle.a, triangle.b, triangle.c];
        for (actual, index) in positions.iter().zip(face) {
            let source = mesh.vertices[*index as usize].position;
            assert_eq!(
                actual[..3],
                [source[0] - 64.0, source[1], source[2] - 128.0]
            );
        }
        assert_ne!(triangle.surface_flags & surface::NO_WIND, 0);
        assert_ne!(triangle.surface_flags & surface::TEXTURE, 0);
        assert_eq!(triangle.normal[3], -1.0);
        cutouts += usize::from(triangle.c[3] == 1.0);
    }
    assert!(
        cutouts > 0 && cutouts < chunk.triangles.len(),
        "actual leaf and bark surfaces must both survive"
    );
}

#[test]
fn malformed_payloads_are_rejected_before_triangle_extraction() {
    let mut mesh = empty();
    mesh.indices = vec![0, 1];
    assert!(extract(&mesh).unwrap_err().contains("complete triangles"));
    mesh.indices.push(2);
    assert!(extract(&mesh).unwrap_err().contains("missing vertex"));
    mesh.indices.clear();
    mesh.key.level = 255;
    assert!(extract(&mesh).unwrap_err().contains("tile bounds"));
}

#[test]
fn disabled_opaque_detail_preserves_actual_cutout_sampling_and_coarse_average() {
    let mut mesh = empty();
    append(&mut mesh, 0, -1, surface());
    append(
        &mut mesh,
        0,
        1,
        Surface {
            cutout: true,
            sample_texture: false,
            ..surface()
        },
    );
    let targets = extract_with_textures(&mesh, false).unwrap();
    assert_eq!(targets.triangles[0].surface_flags & surface::TEXTURE, 0);
    assert_ne!(
        targets.triangles[0].surface_flags & surface::COARSE_COLOR,
        0
    );
    assert_ne!(targets.triangles[1].surface_flags & surface::TEXTURE, 0);
    assert_ne!(
        targets.triangles[1].surface_flags & surface::COARSE_COLOR,
        0
    );
    assert_eq!(targets.triangles[1].c[3], 1.0);
    assert_eq!(targets.triangles[1].a[3], 17.0);
    assert_eq!(
        targets.triangles[0].surface_color,
        targets.triangles[1].surface_color
    );
}

#[test]
fn admitted_tile_medium_retains_only_validated_column_intervals_and_revision() {
    let mesh = empty();
    let mut tile = crate::lod::LodTile {
        key: mesh.key,
        revision: mesh.revision,
        trees: vec![],
        geometric_error: 0,
        columns: vec![
            crate::lod::Column {
                coverage: vec![crate::lod::Interval { bottom: 0, top: 20 }],
                spans: vec![],
            };
            crate::lod::TILE_COLUMNS
        ],
    };
    tile.columns[0].spans.push(crate::lod::Span {
        bottom: 16,
        top: 17,
        state: crate::world::WATER,
        sky: 15,
        glow: 0,
    });
    let ray = extract_tile(&mesh, &tile, crate::content::catalog()).unwrap();
    assert!(ray.key.is_none() && ray.water.is_none());
    let medium = ray.coarse_water.as_ref().unwrap();
    assert_eq!(medium.key, mesh.key);
    assert_eq!(
        medium.columns[0].water,
        vec![crate::lod::Interval {
            bottom: 16,
            top: 17
        }]
    );
    assert_eq!(medium.columns[0].coverage, tile.columns[0].coverage);
    assert!(medium.columns[1].water.is_empty());
    tile.revision += 1;
    assert!(
        extract_tile(&mesh, &tile, crate::content::catalog())
            .unwrap_err()
            .contains("revision")
    );
}

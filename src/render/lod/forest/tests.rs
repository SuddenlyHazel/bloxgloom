use super::*;
use crate::lod::TileKey;

fn empty(key: TileKey) -> Mesh {
    Mesh {
        key,
        revision: 0,
        loading: None,
        ray: None,
        vertices: Vec::new(),
        indices: Vec::new(),
        water_indices: Vec::new(),
        bounds: None,
        coverage: Box::new(crate::render::lod::coverage::Coverage {
            intervals: vec![],
            occupied: vec![],
        }),
    }
}
fn feature(species: u8) -> TreeFeature {
    TreeFeature {
        anchor: [16, 12, 16],
        support_y: Some(13),
        trunk_height: if species == 1 { 13 } else { 9 },
        log: crate::world::WOOD,
        leaves: crate::world::LEAVES,
        branch_x: crate::world::WOOD,
        branch_z: crate::world::WOOD,
        species,
        shape: 0b10100,
    }
}
fn area(mesh: &Mesh) -> f32 {
    mesh.indices
        .chunks_exact(3)
        .map(|triangle| {
            let points: [glam::Vec3; 3] = std::array::from_fn(|i| {
                glam::Vec3::from(mesh.vertices[triangle[i] as usize].position)
            });
            (points[1] - points[0])
                .cross(points[2] - points[0])
                .length()
                * 0.5
        })
        .sum()
}

#[test]
fn server_species_keep_narrow_connected_logs_and_bounded_double_sided_cutout_crowns() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    for species in 0..10 {
        let mut mesh = empty(TileKey {
            level: 4,
            x: 0,
            z: 0,
        });
        append(&mut mesh, &feature(species), catalog, &colors).unwrap();
        assert!(
            mesh.indices.len() / 3 <= 250,
            "species {species}: {} triangles",
            mesh.indices.len() / 3
        );
        assert!(mesh.byte_len() < 16 * 1024);
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.position.iter().all(|p| p.is_finite()))
        );
        let leaves: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|v| {
                let bytes = bytemuck::bytes_of(*v);
                u32::from_ne_bytes(bytes[16..20].try_into().unwrap()) & (1 << 12) != 0
            })
            .collect();
        assert!(!leaves.is_empty());
        let leaf_triangles = mesh
            .indices
            .chunks_exact(3)
            .filter(|tri| {
                let bytes = bytemuck::bytes_of(&mesh.vertices[tri[0] as usize]);
                u32::from_ne_bytes(bytes[16..20].try_into().unwrap()) & (1 << 12) != 0
            })
            .count();
        assert!(leaf_triangles <= 150);
        let ground_ring: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|v| v.position[1] == 13.0)
            .collect();
        assert!(!ground_ring.is_empty());
        assert!(
            ground_ring
                .iter()
                .all(|v| (16.0..=17.0).contains(&v.position[0])
                    && (16.0..=17.0).contains(&v.position[2])),
            "distant stems stay one metre wide"
        );
        for vertex in leaves {
            let bytes = bytemuck::bytes_of(vertex);
            let packed = u32::from_ne_bytes(bytes[16..20].try_into().unwrap());
            assert_ne!(
                (packed >> 13) & 0x3ffff,
                0,
                "all coarse foliage keeps its leaf texture"
            );
            assert_eq!(packed & (1 << 31), 0, "coarse foliage must sample alpha");
        }
        for tri in mesh.indices.chunks_exact(3) {
            let a = mesh.vertices[tri[0] as usize].unpack();
            let b = mesh.vertices[tri[1] as usize].unpack();
            let c = mesh.vertices[tri[2] as usize].unpack();
            let p = |v: [f32; 11]| glam::Vec3::new(v[0], v[1], v[2]);
            let normal = glam::Vec3::new(a[3], a[4], a[5]);
            assert!(
                (p(b) - p(a)).cross(p(c) - p(a)).dot(normal) > 0.0,
                "species {species}: card and bark normals must match backface culling"
            );
        }
    }
}

#[test]
fn crossing_crowns_clip_without_caps_or_geometry_loss_at_negative_tile_seams() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let mut tree = feature(7);
    tree.anchor = [-1, 12, 16];
    let mut full = empty(TileKey {
        level: 1,
        x: -1,
        z: 0,
    });
    // A shifted full tile puts the source tree away from a clipping edge.
    let mut shifted = tree;
    shifted.anchor[0] = -17;
    append(&mut full, &shifted, catalog, &colors).unwrap();
    let mut left = empty(TileKey {
        level: 0,
        x: -1,
        z: 0,
    });
    let mut right = empty(TileKey {
        level: 0,
        x: 0,
        z: 0,
    });
    append(&mut left, &tree, catalog, &colors).unwrap();
    append(&mut right, &tree, catalog, &colors).unwrap();
    for mesh in [&left, &right] {
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.position[0] >= 0.0 && v.position[0] <= 32.0)
        );
    }
    assert!(
        (area(&left) + area(&right) - area(&full)).abs() < 0.001,
        "clipping must preserve existing surfaces without inventing seam roofs"
    );
}

#[test]
fn source_tree_dimensions_and_materials_do_not_expand_with_sample_width() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    for species in [0, 1, 4, 7] {
        let tree = feature(species);
        let mut baseline = empty(TileKey {
            level: 0,
            x: 0,
            z: 0,
        });
        append(&mut baseline, &tree, catalog, &colors).unwrap();
        for level in 1..=4 {
            let mut distant = empty(TileKey { level, x: 0, z: 0 });
            append(&mut distant, &tree, catalog, &colors).unwrap();
            assert_eq!(distant.indices, baseline.indices);
            assert_eq!(
                bytemuck::cast_slice::<Vertex, u8>(&distant.vertices),
                bytemuck::cast_slice::<Vertex, u8>(&baseline.vertices)
            );
        }
    }
}

#[test]
fn polygon_clip_rejects_degenerate_touch_and_owns_coplanar_edges_once() {
    assert!(
        clip(
            &[[32.0, 0.0, 1.0], [32.0, 1.0, 1.0], [32.0, 1.0, 2.0]],
            [32.0, 32.0]
        )
        .is_empty()
    );
    assert!(
        !clip(
            &[[0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 2.0]],
            [32.0, 32.0]
        )
        .is_empty()
    );
    assert!(
        clip(
            &[[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 2.0, 0.0]],
            [32.0, 32.0]
        )
        .is_empty()
    );
}

#[test]
fn exterior_support_moves_only_the_metre_wide_stem_bottom_and_keeps_the_source_crown() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    let tree = feature(7);
    let key = TileKey {
        level: 4,
        x: 0,
        z: 0,
    };
    let mut source = empty(key);
    let mut supported = empty(key);
    append(&mut source, &tree, catalog, &colors).unwrap();
    append_supported(&mut supported, &tree, 10, catalog, &colors).unwrap();
    let leaves = |mesh: &Mesh| {
        mesh.vertices
            .iter()
            .filter(|v| {
                let bytes = bytemuck::bytes_of(*v);
                u32::from_ne_bytes(bytes[16..20].try_into().unwrap()) & (1 << 12) != 0
            })
            .map(|v| v.position.map(f32::to_bits))
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        leaves(&source),
        leaves(&supported),
        "source crown heights and shape are unchanged"
    );
    let roots: Vec<_> = supported
        .vertices
        .iter()
        .filter(|v| v.position[1] == 10.0)
        .collect();
    assert!(!roots.is_empty());
    assert!(
        roots
            .iter()
            .all(|v| (16.0..=17.0).contains(&v.position[0])
                && (16.0..=17.0).contains(&v.position[2]))
    );
    assert!(supported.vertices.iter().all(|v| v.position[1] >= 10.0));
    assert_eq!(tree.anchor, [16, 12, 16]);
    assert!(supported.byte_len() <= source.byte_len() + 512);
}

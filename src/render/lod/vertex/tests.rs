use super::Vertex;
#[test]
fn appearance_bits_do_not_corrupt_normals_light_or_vertex_stride() {
    let v = Vertex::new([0.0; 3], 1, 1, [0.2; 3], 15, 7).material(super::super::surface::Surface {
        color: [0.2, 0.2, 0.2, 0.62],
        layer: Some(511),
        sample_texture: true,
        fluid: true,
        cutout: true,
    });
    assert_eq!(std::mem::size_of::<Vertex>(), 20);
    assert_eq!((v.surface >> 13) & 255, 158);
    assert_ne!(v.surface & (1 << 11), 0);
    assert_ne!(v.surface & (1 << 12), 0);
    assert_eq!(v.unpack()[4], 1.0);
    assert_eq!(v.unpack()[9], 1.0);
    assert_eq!(v.unpack()[10], 7.0 / 15.0);
    assert!(!v.ray_surface().textured);
    assert_eq!(v.ray_surface().layer, 0);
    assert_eq!(v.ray_surface().color[3], 158.0 / 255.0);
}

#[test]
fn dark_ocean_tints_keep_near_surface_precision_without_growing_vertices() {
    let catalog = crate::content::catalog();
    let colors = super::super::FaceColors::new(catalog);
    let surface =
        super::super::surface::Surface::new(catalog, &colors, crate::world::WATER, 1, 1, 4);
    let v = Vertex::new([0.0; 3], 1, 1, [0.0; 3], 15, 0).material(surface);
    let actual = v.ray_surface().color;
    assert_eq!(std::mem::size_of::<Vertex>(), 20);
    for (actual, expected) in actual[..3].iter().zip(surface.color) {
        assert!(actual > &0.0, "dark ocean channels cannot round away");
        assert!((actual - expected).abs() <= 0.000489);
    }
    assert_eq!(&v.unpack()[6..9], &actual[..3]);
    let error = |decoded: [f32; 3]| {
        decoded
            .into_iter()
            .zip(surface.color)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
    };
    let old = surface.color[..3]
        .try_into()
        .map(|rgb: [f32; 3]| rgb.map(|c| (c * 255.0).round() / 255.0))
        .unwrap();
    assert!(
        error(actual[..3].try_into().unwrap()) < error(old) * 0.10,
        "fluid quantization squared error must fall by at least 90%"
    );
    for rgb in [
        [0.0; 3],
        [1.0; 3],
        [0.000001, 0.00001, 0.0001],
        [0.25, 0.5, 1.0],
    ] {
        let decoded = super::fluid_color::decode(super::fluid_color::encode(rgb));
        assert!(
            decoded
                .into_iter()
                .zip(rgb)
                .all(|(a, b)| (a - b).abs() <= 0.001)
        );
    }
}
#[test]
fn compact_layout_preserves_positions_cardinal_normals_and_voxel_light() {
    assert_eq!(std::mem::size_of::<Vertex>(), 20);
    for axis in 0..3 {
        for side in [-1, 1] {
            for sky in 0..=15 {
                for glow in 0..=15 {
                    let v = Vertex::new(
                        [1024.0, -65536.0, 0.5],
                        axis,
                        side,
                        [0.001, 0.32, 0.97],
                        sky,
                        glow,
                    )
                    .unpack();
                    assert_eq!(&v[..3], &[1024.0, -65536.0, 0.5]);
                    let mut normal = [0.0; 3];
                    normal[axis] = side as f32;
                    assert_eq!(&v[3..6], &normal);
                    assert_eq!(v[9], f32::from(sky) / 15.0);
                    assert_eq!(v[10], f32::from(glow) / 15.0);
                    for (actual, expected) in v[6..9].iter().zip([0.001, 0.32, 0.97]) {
                        assert!((actual - expected).abs() <= 0.5 / 255.0);
                    }
                }
            }
        }
    }
}

#[test]
fn coarse_reference_material_identity_does_not_enable_texture_sampling() {
    let v = Vertex::new([0.0; 3], 1, 1, [0.2; 3], 15, 7).material(super::super::surface::Surface {
        color: [0.2, 0.2, 0.2, 1.0],
        layer: Some(4095),
        sample_texture: false,
        fluid: false,
        cutout: true,
    });
    assert_eq!((v.surface >> 13) & 0x3ffff, 4096);
    assert_ne!(v.surface & 0x80000000, 0);
    assert_eq!(v.unpack()[4], 1.0);
    assert_eq!(v.unpack()[9], 1.0);
    assert_eq!(v.unpack()[10], 7.0 / 15.0);
}

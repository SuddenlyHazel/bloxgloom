use super::Vertex;
#[test]
fn appearance_bits_do_not_corrupt_normals_light_or_vertex_stride() {
    let v = Vertex::new([0.0; 3], 1, 1, [0.2; 3], 15, 7).material(super::super::surface::Surface {
        color: [0.2, 0.2, 0.2, 0.62],
        layer: Some(511),
        fluid: true,
        cutout: true,
    });
    assert_eq!(std::mem::size_of::<Vertex>(), 20);
    assert_eq!(v.surface >> 13, 512);
    assert_ne!(v.surface & (1 << 11), 0);
    assert_ne!(v.surface & (1 << 12), 0);
    assert_eq!(v.unpack()[4], 1.0);
    assert_eq!(v.unpack()[9], 1.0);
    assert_eq!(v.unpack()[10], 7.0 / 15.0);
    assert!(v.color[3] < 255);
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

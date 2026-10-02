use super::*;
use std::borrow::Cow;
mod gpu;

fn register(catalog: &mut Catalog, key: &str, pixels: &[u8]) {
    let mut bytes = vec![];
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(pixels)
            .unwrap();
    }
    catalog
        .register_texture(TextureDef {
            key: key.to_owned().into(),
            png: Cow::Owned(bytes),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: false,
            emission_strength: 0.0,
        })
        .unwrap();
}

#[test]
fn companion_keys_resolve_independently_of_registration_order_and_missing_maps_fall_back() {
    let mut catalog = Catalog::new();
    register(&mut catalog, "test:rock_s", &[64, 0, 0, 255].repeat(4));
    register(&mut catalog, "test:rock_n", &[128, 128, 255, 17].repeat(4));
    register(&mut catalog, "test:rock", &[170, 150, 120, 255].repeat(4));
    register(&mut catalog, "test:plain", &[170, 150, 120, 255].repeat(4));
    let maps = prepare(&catalog);
    assert_eq!(maps.flags, vec![0, 0, 3, 0]);
    let stride = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
    assert_eq!(
        &maps.normal[0][2 * stride..2 * stride + 4],
        &[128, 128, 255, 17]
    );
    assert_eq!(
        &maps.specular[0][2 * stride..2 * stride + 4],
        &[64, 0, 0, 255]
    );
    assert_eq!(
        &maps.normal[0][3 * stride..3 * stride + 4],
        &[128, 128, 255, 255]
    );
    assert_eq!(
        &maps.specular[0][3 * stride..3 * stride + 4],
        &[0, 0, 0, 255]
    );
    assert_eq!(maps.normal.len(), TEXTURE_MIPS as usize);
    assert_eq!(&maps.normal.last().unwrap()[8..12], &[128, 128, 255, 17]);
}

#[test]
fn data_mips_are_linear_and_do_not_weight_normals_by_height_alpha() {
    let mut catalog = Catalog::new();
    register(&mut catalog, "test:rock", &[170, 150, 120, 255].repeat(4));
    register(
        &mut catalog,
        "test:rock_n",
        &[
            80, 128, 240, 0, 176, 128, 240, 255, 80, 128, 240, 0, 176, 128, 240, 255,
        ],
    );
    let maps = prepare(&catalog);
    assert_eq!(&maps.normal.last().unwrap()[..4], &[128, 128, 240, 127]);
    let old = catalog.fingerprint();
    register(&mut catalog, "test:rock_s", &[64, 0, 0, 255].repeat(4));
    assert_ne!(catalog.fingerprint(), old);
}

#[test]
fn builtin_terrain_companions_are_registered_without_changing_original_layers() {
    let catalog = Catalog::builtins();
    let maps = prepare(&catalog);
    assert_eq!(&maps.flags[..9], &[3; 9]);
    assert!(maps.flags[9..].iter().all(|&f| f == 0));
    assert_eq!(catalog.textures()[3].key, "bloxgloom:stone");
    assert_eq!(catalog.textures()[27].key, "bloxgloom:grass_top_n");
}

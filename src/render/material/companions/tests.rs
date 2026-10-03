use super::*;
use std::borrow::Cow;
mod gpu;
mod parallax;

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
            foliage: Default::default(),
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
fn builtin_companions_are_registered_without_changing_original_layers() {
    let catalog = Catalog::builtins();
    let maps = prepare(&catalog);
    assert_eq!(&maps.flags[..11], &[3; 11]);
    assert!(maps.flags[11..19].iter().all(|&f| f & 3 == 0));
    assert_eq!(maps.flags[19], 3);
    assert!(maps.flags[20..].iter().all(|&f| f == 0));
    assert_eq!(catalog.textures()[3].key, "bloxgloom:stone");
    assert_eq!(catalog.textures()[27].key, "bloxgloom:grass_top_n");
    assert_eq!(catalog.textures()[45].key, "bloxgloom:wood_side_n");
}

#[test]
fn foliage_metadata_is_opt_in_bounded_and_fingerprinted() {
    use bloxgloom_host_api::content::FoliageShading;
    let mut plain = Catalog::new();
    register(&mut plain, "test:leaf", &[100, 150, 50, 255].repeat(4));
    let original = plain.textures()[0].clone();
    let mut changed = original.clone();
    changed.foliage = FoliageShading {
        wrap: 0.35,
        transmission: 0.28,
    };
    let mut foliage = Catalog::new();
    foliage.register_texture(changed).unwrap();
    assert_ne!(plain.fingerprint(), foliage.fingerprint());
    assert_eq!(prepare(&plain).flags, [0]);
    assert_eq!(prepare(&foliage).flags, [89 << 8 | 71 << 16]);
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.01] {
        for transmission in [false, true] {
            let mut bad = original.clone();
            if transmission {
                bad.foliage.transmission = value;
            } else {
                bad.foliage.wrap = value;
            }
            assert!(Catalog::new().register_texture(bad).is_err());
        }
    }
    let builtins = Catalog::builtins();
    for texture in builtins.textures() {
        if matches!(
            texture.key.as_ref(),
            "bloxgloom:leaves"
                | "bloxgloom:flower_red"
                | "bloxgloom:flower_yellow"
                | "bloxgloom:flower_blue"
                | "bloxgloom:fern"
                | "bloxgloom:tall_grass"
        ) {
            assert!(texture.foliage.wrap > 0.0);
            assert!(texture.foliage.transmission > 0.0);
        } else {
            assert_eq!(texture.foliage, FoliageShading::default());
        }
    }
}

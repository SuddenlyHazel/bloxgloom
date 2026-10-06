use super::*;
use std::borrow::Cow;
mod gpu;

mod optics;
mod parallax;
mod pbr;
mod sampling;

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
    assert_eq!(maps.layers, vec![0, 0, 0, 1]);
    let stride = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
    assert_eq!(&maps.normal[0][..4], &[128, 128, 255, 17]);
    assert_eq!(&maps.specular[0][..4], &[64, 0, 0, 255]);
    assert_eq!(&maps.normal[0][stride..stride + 4], &[128, 128, 255, 255]);
    assert_eq!(&maps.specular[0][stride..stride + 4], &[0, 0, 0, 255]);
    assert_eq!(maps.normal.len(), TEXTURE_MIPS as usize);
    assert_eq!(&maps.normal.last().unwrap()[..4], &[128, 128, 255, 17]);
}

#[test]
fn normal_mips_average_directions_without_height_coverage_weighting() {
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
    let last = &maps.normal.last().unwrap()[..4];
    assert!((i16::from(last[0]) - 128).abs() <= 1);
    assert!((i16::from(last[1]) - 128).abs() <= 1);
    assert_eq!(&last[2..], &[255, 127]);
    assert_ne!(
        maps.flags[0] & 32,
        0,
        "variable opaque height enables tracing"
    );
    let old = catalog.fingerprint();
    register(&mut catalog, "test:rock_s", &[64, 0, 0, 255].repeat(4));
    assert_ne!(catalog.fingerprint(), old);
}

#[test]
fn lab_pbr_emission_sentinel_is_removed_before_mip_filtering() {
    let mut catalog = Catalog::new();
    register(&mut catalog, "test:rock", &[170, 150, 120, 255].repeat(4));
    register(
        &mut catalog,
        "test:rock_s",
        &[
            64, 10, 19, 255, 64, 10, 19, 254, 64, 10, 19, 255, 64, 10, 19, 254,
        ],
    );
    catalog.mark_lab_pbr_texture(crate::content::TextureId::new(0));
    let maps = prepare(&catalog);
    assert_eq!(maps.flags[0] & 18, 18);
    assert_eq!(maps.specular[0][3], 0);
    assert_eq!(maps.specular[0][(TEXTURE_SIZE * 4 - 1) as usize], 254);
    assert_eq!(maps.specular.last().unwrap()[3], 127);
}

#[test]
fn lab_pbr_mips_keep_categories_and_filter_continuous_channels() {
    let mut catalog = Catalog::new();
    register(&mut catalog, "test:ore", &[170, 150, 120, 255].repeat(4));
    register(
        &mut catalog,
        "test:ore_s",
        &[
            0, 231, 255, 254, 128, 10, 19, 255, 128, 10, 19, 255, 0, 231, 255, 254,
        ],
    );
    catalog.mark_lab_pbr_texture(TextureId::new(0));
    let maps = prepare(&catalog);
    for level in &maps.specular {
        for pixel in level.chunks_exact(4) {
            assert!(matches!(pixel[1], 10 | 231), "invented material ID");
            assert!(matches!(pixel[2], 19 | 255), "invented porosity/SSS value");
        }
    }
    let last = &maps.specular.last().unwrap()[..4];
    // Roughness filters in squared space to retain the unresolved lobe width.
    let expected =
        ((1.0 - ((1.0 + (127.0_f32 / 255.0).powi(2)) * 0.5).sqrt()) * 255.0).round() as u8;
    assert_eq!(last, [expected, 231, 255, 127]);
}

#[test]
fn unresolved_lab_normal_variance_broadens_reflections_and_preserves_ao_height() {
    let stride = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
    let mut normal = Vec::with_capacity(stride);
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            normal.extend([
                if (x + y) % 2 == 0 { 51 } else { 204 },
                128,
                211,
                if x % 2 == 0 { 255 } else { 127 },
            ]);
        }
    }
    let source = normal.clone();
    let specular = [230, 10, 20, 0].repeat(stride / 4);
    let (normals, materials) = normal_mips::prepare(normal, specular.clone(), &[true]);
    assert_eq!(normals[0], source, "authored base normal stays unchanged");
    assert_eq!(
        materials[0], specular,
        "authored base material stays unchanged"
    );
    for level in 1..TEXTURE_MIPS as usize {
        let n = &normals[level][..4];
        assert!((i16::from(n[0]) - 128).abs() <= 1);
        assert_eq!(
            &n[2..],
            &[211, 191],
            "AO and height remain independent scalar data"
        );
        let s = &materials[level][..4];
        assert!(
            s[0] < 140,
            "unresolved tilted normals must not become a glossy flat surface"
        );
        assert_eq!(&s[1..], &[10, 20, 0]);
        assert_eq!(
            s[0], materials[1][0],
            "variance must survive repeated normalization"
        );
    }
    let flat = [128, 128, 211, 255].repeat(stride / 4);
    let (_, smooth) = normal_mips::prepare(flat, specular, &[true]);
    assert_eq!(
        smooth.last().unwrap()[0],
        230,
        "a constant normal does not become rougher"
    );
}

#[test]
fn cutout_height_does_not_enable_camera_or_light_ray_marching() {
    let mut catalog = Catalog::new();
    register(&mut catalog, "test:leaf", &[80, 160, 40, 255].repeat(4));
    register(
        &mut catalog,
        "test:leaf_n",
        &[
            128, 128, 255, 0, 128, 128, 255, 255, 128, 128, 255, 0, 128, 128, 255, 255,
        ],
    );
    let mut leaf = catalog.textures()[0].clone();
    leaf.alpha_cutout = true;
    let mut cutout = Catalog::new();
    cutout.register_texture(leaf).unwrap();
    cutout
        .register_texture(catalog.textures()[1].clone())
        .unwrap();
    assert_eq!(prepare(&cutout).flags[0] & 32, 0);
}

#[test]
fn builtin_companions_are_registered_without_changing_original_layers() {
    let catalog = Catalog::builtins();
    let maps = prepare(&catalog);
    assert!(maps.flags[..11].iter().all(|flags| flags & 3 == 3));

    assert_eq!(maps.flags[19], 3);
    for (id, texture) in catalog.textures().iter().enumerate() {
        let exists = |suffix: &str| {
            catalog
                .textures()
                .iter()
                .any(|candidate| candidate.key.as_ref() == format!("{}{suffix}", texture.key))
        };
        let expected = u32::from(exists("_n")) | (u32::from(exists("_s")) << 1);
        assert_eq!(maps.flags[id] & 3, expected, "{}", texture.key);
    }
    assert!(super::super::texture_layers_for(&catalog) < catalog.textures().len() as u32);
    assert_eq!(catalog.textures()[3].key, "bloxgloom:stone");
    assert_eq!(catalog.textures()[27].key, "bloxgloom:grass_top_n");
    assert_eq!(catalog.textures()[45].key, "bloxgloom:wood_side_n");
    for (key, expected) in [
        ("jg_large_fern_bottom", 72),
        ("jg_large_fern_top", 200),
        ("jg_amethyst_cluster", 0),
    ] {
        let id = catalog
            .textures()
            .iter()
            .position(|t| t.key.as_ref() == format!("bloxgloom:{key}"))
            .unwrap();
        assert_eq!(
            maps.flags[id] & (8 | 64 | 128),
            expected,
            "{key} botanical wind/paired-half metadata"
        );
    }
    for (key, thickness) in [
        ("jg_cherry_leaves", 4),
        ("jg_oak_leaves", 7),
        ("jg_spruce_leaves", 13),
        ("jg_large_fern_bottom", 5),
        ("jg_amethyst_cluster", 0),
    ] {
        let id = catalog
            .textures()
            .iter()
            .position(|t| t.key.as_ref() == format!("bloxgloom:{key}"))
            .unwrap();
        assert_eq!(
            (maps.flags[id] >> 27) & 31,
            thickness,
            "{key} optical path coefficient"
        );
        assert_eq!(
            maps.flags[id] & (7 << 24),
            0,
            "botanical thickness must not corrupt auxiliary sampling"
        );
    }
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
    for texture in &builtins.textures()[..27] {
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

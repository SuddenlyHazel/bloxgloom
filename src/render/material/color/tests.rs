use super::*;

#[test]
fn mip_averages_light_and_not_srgb_bytes() {
    let black = [0, 0, 0, 255];
    let white = [255; 4];
    assert_eq!(
        downsample([&black, &white, &black, &white]),
        [188, 188, 188, 255]
    );
    let red = [255, 0, 0, 255];
    let blue = [0, 0, 255, 255];
    assert_eq!(downsample([&red, &blue, &red, &blue]), [188, 0, 188, 255]);
    assert_eq!(blend(0, 255, 4, 4), (188, 188));
}

#[test]
fn mip_weights_light_by_alpha_without_hidden_color_bleed() {
    let visible = [200, 80, 30, 255];
    let hidden = [0, 0, 255, 0];
    assert_eq!(
        downsample([&visible, &hidden, &visible, &hidden]),
        [200, 80, 30, 127]
    );
    assert_eq!(downsample([&hidden; 4]), [0; 4]);
    let faint_white = [255, 255, 255, 85];
    let black = [0, 0, 0, 255];
    assert_eq!(
        downsample([&black, &faint_white, &black, &faint_white]),
        [137, 137, 137, 170]
    );
}

#[test]
fn constant_albedo_survives_every_byte_value_and_mip_level() {
    for channel in 0..=255 {
        let pixel = [channel, channel, channel, 255];
        let mut filtered = pixel;
        for _ in 0..8 {
            filtered = downsample([&filtered; 4]);
            assert_eq!(filtered, pixel);
        }
    }
}

#[test]
fn catalog_albedo_mip_chain_preserves_linear_light_average() {
    use crate::content::{Catalog, TextureDef};
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[
                0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255,
            ])
            .unwrap();
    }
    let mut catalog = Catalog::new();
    catalog
        .register_texture(TextureDef {
            key: "test:linear_average".into(),
            png: std::borrow::Cow::Owned(bytes),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: false,
            emission_strength: 0.0,
            foliage: Default::default(),
        })
        .unwrap();
    let levels = super::super::material_mips_for(&catalog);
    assert_eq!(levels.last().unwrap(), &[188, 188, 188, 255]);
}

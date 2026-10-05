use super::*;
use crate::content::{ItemDef, ItemId, TextureDef};
use std::borrow::Cow;

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[128, 128, 255, 255])
            .unwrap();
    }
    bytes
}

fn register(catalog: &mut Catalog, key: &str, png: &[u8]) {
    catalog
        .register_texture(TextureDef {
            key: key.to_owned().into(),
            png: Cow::Owned(png.to_vec()),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: false,
            emission_strength: 0.0,
            foliage: Default::default(),
        })
        .unwrap();
}

#[test]
fn nine_hundred_catalog_textures_admit_three_hundred_full_quality_materials() {
    let mut catalog = Catalog::new();
    let png = png();
    for id in 0..300 {
        // Companion registration order does not define GPU packing.
        for suffix in ["_n", "_s", ""] {
            register(&mut catalog, &format!("test:material{id}{suffix}"), &png);
        }
    }
    let layers = Layers::new(&catalog);
    assert_eq!(catalog.textures().len(), 900);
    assert_eq!(layers.definitions.len(), 300);
    for id in 0..300 {
        assert_eq!(&layers.by_texture[id * 3..id * 3 + 3], &[id as u32; 3]);
    }
    let usage = super::super::resources::validate(layers.definitions.len(), 512).unwrap();
    assert!(usage.mip_bytes < super::super::resources::MAX_ARRAY_BYTES);
}

#[test]
fn standalone_suffixes_and_explicit_companion_artwork_keep_their_own_layer() {
    let mut catalog = Catalog::new();
    let png = png();
    for key in ["test:rock_n", "test:rock", "test:standalone_s"] {
        register(&mut catalog, key, &png);
    }
    assert_eq!(Layers::new(&catalog).by_texture, [0, 0, 1]);
    catalog
        .register_item(ItemDef {
            id: ItemId::new(1),
            key: "test:normal_preview".into(),
            name: "Normal preview".into(),
            swatch: [1.0; 4],
            texture: TextureId::new(0),
            placeable: None,
            sprite: true,
        })
        .unwrap();
    let layers = Layers::new(&catalog);
    assert_eq!(layers.definitions, [0, 1, 2]);
    assert_eq!(layers.by_texture, [0, 1, 2]);
    let pixels = super::super::material_tiles_for(&catalog);
    assert_eq!(&pixels[..4], &[128, 128, 255, 255]);
}

#[test]
fn cutout_mips_keep_coverage_and_do_not_bleed_invisible_colors() {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[
                200, 80, 30, 255, 0, 0, 255, 0, 200, 80, 30, 255, 0, 0, 255, 0,
            ])
            .unwrap();
    }
    let mut catalog = Catalog::new();
    catalog
        .register_texture(TextureDef {
            key: "test:cutout".into(),
            png: Cow::Owned(bytes),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: true,
            emission_strength: 0.0,
            foliage: Default::default(),
        })
        .unwrap();
    let mips = super::super::material_mips_for(&catalog);
    assert_eq!(&mips[0][..4], &[200, 80, 30, 255]);
    assert_eq!(
        &mips[0][(super::super::TEXTURE_SIZE * 4 - 4) as usize..][..4],
        &[0, 0, 255, 0]
    );
    assert_eq!(mips.last().unwrap(), &[200, 80, 30, 127]);
}

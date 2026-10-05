//! Real imported art must honor the source contract, not just synthetic fixtures.
use super::*;

fn pixels(catalog: &Catalog, key: &str) -> Vec<u8> {
    let id = catalog.texture_key(&format!("bloxgloom:{key}")).unwrap();
    let texture = catalog.texture(id).unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(texture.png.as_ref()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    assert_eq!((frame.width, frame.height), (256, 256));
    bytes.truncate(frame.buffer_size());
    bytes
}

#[test]
fn imported_ordinary_and_deepslate_ores_use_the_same_declared_metal() {
    let catalog = Catalog::builtins();
    for (key, metal) in [
        ("jg_iron_ore_s", 230),
        ("jg_deepslate_iron_ore_s", 230),
        ("jg_copper_ore_s", 234),
        ("jg_deepslate_copper_ore_s", 234),
        ("jg_gold_ore_s", 231),
        ("jg_deepslate_gold_ore_s", 231),
        ("jg_nether_gold_ore_s", 231),
    ] {
        let bytes = pixels(&catalog, key);
        assert!(
            bytes.chunks_exact(4).any(|p| p[1] == metal),
            "{key} lost its metal"
        );
        assert!(
            bytes.chunks_exact(4).any(|p| p[1] == 10),
            "{key} lost its stone"
        );
        assert!(
            bytes.chunks_exact(4).all(|p| p[1] == 10 || p[1] == metal),
            "{key} has incorrect F0 or metal IDs"
        );
    }
    for key in ["stone_s", "dirt_s", "wood_side_s", "jg_cobblestone_s"] {
        assert!(
            pixels(&catalog, key).chunks_exact(4).all(|p| p[1] == 10),
            "{key} dielectric F0"
        );
    }
}

#[test]
fn imported_normal_slopes_match_canonical_source_pixels() {
    let catalog = Catalog::builtins();
    // Independently audited Bedrock source XY and authored Java AO/height.
    // Gravel is a Java green-inversion exception; it needs the same convention.
    for (key, expected) in [
        ("stone_n", [149, 169, 242, 255]),
        ("dirt_n", [212, 89, 242, 255]),
        ("wood_side_n", [12, 116, 252, 255]),
        ("gravel_n", [180, 125, 225, 249]),
        ("jg_cobblestone_n", [131, 129, 235, 255]),
    ] {
        let bytes = pixels(&catalog, key);
        let offset = (23 * 256 + 19) * 4;
        assert_eq!(
            &bytes[offset..offset + 4],
            &expected,
            "{key} source convention"
        );
    }
}

#[test]
fn imported_smoothness_matches_perceptual_mer_source_pixels() {
    let catalog = Catalog::builtins();
    // Canonical MER blue at (19,23): 76, 203, and 183 respectively.
    // Includes a Bedrock fallback and a Java export with the legacy curve.
    for (key, expected) in [
        ("jg_amethyst_cluster_s", 179),
        ("jg_pale_oak_log_s", 52),
        ("jg_deepslate_iron_ore_s", 72),
    ] {
        let bytes = pixels(&catalog, key);
        assert_eq!(
            bytes[(23 * 256 + 19) * 4],
            expected,
            "{key} perceptual smoothness"
        );
    }
}

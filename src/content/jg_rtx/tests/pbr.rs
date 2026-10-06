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
    // Independently audited Bedrock source XY and authored Java AO. Height has
    // a separate contract: authored relief is retained and selected flat maps
    // receive explicitly identified, confidence-gated inferred relief.
    // Gravel is a Java green-inversion exception; it needs the same convention.
    for (key, expected) in [
        ("stone_n", [149, 169, 242]),
        ("dirt_n", [212, 89, 242]),
        ("wood_side_n", [12, 116, 252]),
        ("gravel_n", [180, 125, 225]),
        ("jg_cobblestone_n", [131, 129, 235]),
    ] {
        let bytes = pixels(&catalog, key);
        let offset = (23 * 256 + 19) * 4;
        assert_eq!(
            &bytes[offset..offset + 3],
            &expected,
            "{key} source convention"
        );
    }
}

#[test]
fn imported_height_preserves_authored_samples_and_identifies_inferred_relief() {
    let catalog = Catalog::builtins();
    let offset = (23 * 256 + 19) * 4 + 3;
    // Existing source-provided height stays exact, including a flat dirt map.
    // The two terracotta values come from the declared Bedrock heightmaps.
    for (key, expected) in [
        ("stone_n", 255),
        ("dirt_n", 255),
        ("gravel_n", 249),
        ("jg_cobblestone_n", 255),
        ("jg_yellow_glazed_terracotta_n", 223),
        ("jg_brown_glazed_terracotta_n", 172),
    ] {
        assert_eq!(
            pixels(&catalog, key)[offset],
            expected,
            "{key} authored height"
        );
    }
    let provenance: serde_json::Value =
        serde_json::from_str(include_str!("../../../../assets/jg-rtx/provenance.json")).unwrap();
    let conversion = |key: &str| {
        let destination = format!("/{key}.png");
        &provenance
            .as_array()
            .unwrap()
            .iter()
            .find(|record| {
                record["destination"]
                    .as_str()
                    .unwrap()
                    .ends_with(&destination)
            })
            .unwrap()["pbr_conversion"]
    };
    // Audited source-based reconstruction samples and extrema guard physical
    // scale as well as orientation. Flattening or arbitrary min/max stretching
    // would fail even if the provenance still claimed successful integration.
    for (key, sample, minimum, minimum_pixel, maximum_pixel, fit, depth) in [
        (
            "wood_side",
            243,
            206,
            (253, 184),
            (187, 224),
            0.811964,
            0.048186,
        ),
        (
            "jg_cherry_log",
            220,
            203,
            (178, 101),
            (32, 97),
            0.850245,
            0.050588,
        ),
        (
            "jg_cherry_planks",
            249,
            228,
            (160, 32),
            (130, 2),
            0.997064,
            0.026272,
        ),
        (
            "jg_bricks",
            240,
            211,
            (95, 222),
            (101, 68),
            0.984291,
            0.043294,
        ),
    ] {
        let bytes = pixels(&catalog, &format!("{key}_n"));
        let alpha = |(x, y): (usize, usize)| bytes[(y * 256 + x) * 4 + 3];
        assert_eq!(bytes[offset], sample, "{key} inferred height sample");
        assert_eq!(alpha(minimum_pixel), minimum, "{key} groove depth");
        assert_eq!(alpha(maximum_pixel), 255, "{key} surface peak");
        assert_eq!(bytes.chunks_exact(4).map(|p| p[3]).min(), Some(minimum));
        let details = conversion(key);
        assert_eq!(
            details["height_source_kind"],
            "inferred from canonical RGB normal; not authored height"
        );
        let measurement = &details["height_reconstruction"];
        assert!((measurement["normal_fit"].as_f64().unwrap() - fit).abs() < 0.000001);
        assert!((measurement["depth_fraction"].as_f64().unwrap() - depth).abs() < 0.000001);
        assert!(
            details.get("height_source").is_none(),
            "inference cannot claim an authored height source"
        );
    }
    // Poorly integrable source normals keep their authored flat alpha.
    for key in ["jg_acacia_log", "jg_jungle_log_top"] {
        assert!(
            pixels(&catalog, &format!("{key}_n"))
                .chunks_exact(4)
                .all(|p| p[3] == 255)
        );
        let details = conversion(key);
        assert!(details.get("height_source_kind").is_none());
        assert!(
            details["height_reconstruction"]["normal_fit"]
                .as_f64()
                .unwrap()
                < 0.70
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

#[test]
fn curated_botanical_materials_are_dry_and_provenance_distinguishes_derived_art() {
    let catalog = Catalog::builtins();
    for (key, ceiling) in [
        ("jg_cherry_leaves_s", 128),
        ("jg_oak_leaves_s", 128),
        ("jg_poppy_s", 153),
    ] {
        let bytes = pixels(&catalog, key);
        assert!(bytes.chunks_exact(4).all(|p| p[0] <= ceiling));
        assert!(bytes.chunks_exact(4).any(|p| p[0] == ceiling));
    }
    let crystal = catalog
        .texture(
            catalog
                .texture_key("bloxgloom:jg_amethyst_cluster")
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        crystal.foliage,
        Default::default(),
        "mineral cutouts do not bend or transmit as plants"
    );
    let provenance: serde_json::Value =
        serde_json::from_str(include_str!("../../../../assets/jg-rtx/provenance.json")).unwrap();
    for key in ["jg_cherry_log", "jg_oak_log", "wood_side"] {
        let record = provenance
            .as_array()
            .unwrap()
            .iter()
            .find(|v| {
                v["destination"]
                    .as_str()
                    .unwrap()
                    .ends_with(&format!("/{key}.png"))
            })
            .unwrap();
        assert_eq!(
            record["art_curation"]["albedo"]["kind"],
            "derived diffuse art"
        );
        assert_eq!(
            record["art_curation"]["albedo"]["tool"],
            "tools/jg_rtx/art.py"
        );
    }
}

use super::*;

const SHADER: &str =
    include_str!("../../../../../../fixtures/material-packages/jade/assets/shaders/jade.wgsl");
const DESCRIPTOR: &str = r#"{"shader":"jade","texture":"bloxgloom:stone"}"#;

#[test]
fn version_two_material_verifies_hooks_parameters_and_negotiated_layers() {
    let descriptor = r#"{"version":2,"shader":"jade","targets":["bloxgloom:stone","bloxgloom:dirt"],"textures":["bloxgloom:stone","bloxgloom:dirt"],"parameters":[{"name":"gain","kind":"float","default":0.5,"min":0,"max":1}],"vertex_offset":0.1}"#;
    let shader = "fn material_fragment(input: BgSurface) -> BgSurface { var result=input; result.albedo=material_texture(input.uv,1u)*material_parameter(0u).x; return result; }";
    let bytes = bundle(&["jade"], descriptor, shader, false);
    let verified = ClientBundle::decode_verify(&bytes, key(&bytes)).unwrap();
    let catalog = crate::content::Catalog::builtins();
    let material = verified.material().unwrap().resolve(&catalog).unwrap();
    assert_eq!(material.materials[0].layers, vec![3, 2]);
    let mut state = verified.parameter_state().unwrap();
    state
        .apply(
            "jade",
            &[crate::render::parameters::Update {
                resource: "jade:tint".into(),
                name: "gain".into(),
                value: crate::render::parameters::Value::Scalar(0.75),
            }],
        )
        .unwrap();
    for descriptor in [
        descriptor.replace("\"version\":2", "\"version\":3"),
        descriptor.replace("\"vertex_offset\":0.1", "\"vertex_offset\":0.3"),
        descriptor.replace("\"default\":0.5", "\"default\":2"),
        descriptor.replace("bloxgloom:dirt", "other:dirt"),
    ] {
        let bad = bundle(&["jade"], &descriptor, shader, false);
        assert!(ClientBundle::decode_verify(&bad, key(&bad)).is_err());
    }
    for shader in [
        shader.replace("return result;", "loop {} return result;"),
        shader.replace("BgSurface", "BgVertex"),
        format!("{shader}\n@group(3) @binding(0) var<uniform> foreign:vec4f;"),
    ] {
        let bad = bundle(&["jade"], descriptor, &shader, false);
        assert!(ClientBundle::decode_verify(&bad, key(&bad)).is_err());
    }
}

fn bundle(packages: &[&str], descriptor: &str, shader: &str, extra: bool) -> Vec<u8> {
    let mut writer = header(packages.len());
    for package in packages {
        writer.field(package.as_bytes()).unwrap();
        writer.field(b"1.0.0").unwrap();
        writer.count(0).unwrap(); // dependencies
        writer.count(0).unwrap(); // modules
        writer.count(if extra { 3 } else { 2 }).unwrap();
        for (name, kind, bytes) in [("jade", 9, shader), ("tint", 8, descriptor)] {
            writer.field(name.as_bytes()).unwrap();
            writer.count(kind).unwrap();
            writer.field(bytes.as_bytes()).unwrap();
        }
        if extra {
            writer.field(b"unused").unwrap();
            writer.count(9).unwrap();
            writer.field(shader.as_bytes()).unwrap();
        }
    }
    writer.count(0).unwrap(); // startup absent
    writer.0
}

#[test]
fn material_bundle_verifies_key_ownership_limits_and_catalog_readiness() {
    let bytes = bundle(&["jade"], DESCRIPTOR, SHADER, false);
    let verified = ClientBundle::decode_verify(&bytes, key(&bytes)).unwrap();
    let resolved = verified
        .material()
        .unwrap()
        .resolve(&crate::content::Catalog::builtins())
        .unwrap();
    assert_eq!(resolved.materials[0].owner, "jade:tint");
    for descriptor in [
        DESCRIPTOR.replace("bloxgloom:stone", "other:stone"),
        DESCRIPTOR.replace("bloxgloom:stone", "jade:missing"),
        DESCRIPTOR.replace("\"jade\"", "\"tint\""),
        DESCRIPTOR.replace("\"jade\"", "\"foreign:jade\""),
        format!("{DESCRIPTOR} {{}}"),
        " ".repeat(1025),
    ] {
        let bad = bundle(&["jade"], &descriptor, SHADER, false);
        match ClientBundle::decode_verify(&bad, key(&bad)) {
            Ok(bundle) => assert!(
                bundle
                    .material()
                    .unwrap()
                    .resolve(&crate::content::Catalog::builtins())
                    .is_err()
            ),
            Err(error) => assert!(format!("{error:?}").contains("jade")),
        }
    }
    for shader in [
        "not wgsl".to_owned(),
        " ".repeat(crate::render::custom::MAX_SHADER_BYTES + 1),
    ] {
        let bad = bundle(&["jade"], DESCRIPTOR, &shader, false);
        assert!(ClientBundle::decode_verify(&bad, key(&bad)).is_err());
    }
    for bad in [
        bundle(&["jade", "zebra"], DESCRIPTOR, SHADER, false),
        bundle(&["jade"], DESCRIPTOR, SHADER, true),
    ] {
        assert!(ClientBundle::decode_verify(&bad, key(&bad)).is_err());
    }
    let mut v5 = bytes.clone();
    v5[8] = 5;
    assert!(ClientBundle::decode_verify(&v5, key(&v5)).is_err());
    assert_ne!(verified.cache_key(), key(&v5));
}

#[test]
fn format_two_fixture_discovers_and_resolves_material_without_effect_or_ui() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/material-packages");
    let snapshot = crate::server::PackageSnapshot::discover(&root).unwrap();
    assert!(
        snapshot
            .client_bundle()
            .material()
            .unwrap()
            .resolve(&crate::content::Catalog::builtins())
            .is_err()
    );
    let startup = crate::server::script::startup::Declarations::discover(&root).unwrap();
    let verified = ClientBundle::decode_verify(
        startup.client_bundle.bytes(),
        startup.client_bundle.cache_key(),
    )
    .unwrap();
    let catalog = verified.session_catalog().unwrap();
    assert_eq!(verified.material().unwrap().materials[0].owner, "jade:tint");
    assert!(verified.effect().is_none() && verified.ui().is_none());
    assert_eq!(
        verified.packages()["jade"].textures["tile"],
        include_bytes!(
            "../../../../../../fixtures/material-packages/jade/assets/textures/jade.png"
        )
    );
    assert_eq!(catalog.textures().last().unwrap().key.as_ref(), "jade:tile");
    verified.material().unwrap().resolve(&catalog).unwrap();
    let item = catalog.item_by_key("jade:token").unwrap();
    assert_eq!(
        catalog
            .texture(catalog.item(item).unwrap().texture)
            .unwrap()
            .key
            .as_ref(),
        "jade:tile"
    );
    let mut manifest = crate::content::ContentManifest::from_catalog(&catalog);
    manifest
        .entries
        .iter_mut()
        .find(|entry| entry.kind == b'I' && entry.key == "jade:token")
        .unwrap()
        .id = 65_536;
    manifest
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    let remapped = manifest.resolve_catalog(&catalog).unwrap();
    let remapped_item = remapped.item_by_key("jade:token").unwrap();
    assert_eq!(remapped_item.get(), 65_536);
    assert_eq!(
        remapped
            .texture(remapped.item(remapped_item).unwrap().texture)
            .unwrap()
            .key
            .as_ref(),
        "jade:tile"
    );
    verified.material().unwrap().resolve(&remapped).unwrap();
}

#[test]
fn material_effect_and_ui_assets_coexist_in_canonical_bundle() {
    let mut writer = header(1);
    writer.field(b"jade").unwrap();
    writer.field(b"1.0.0").unwrap();
    writer.count(0).unwrap(); // dependencies
    writer.count(0).unwrap(); // modules
    let effect =
        include_str!("../../../../../../fixtures/effect-packages/sepia/assets/shaders/sepia.wgsl");
    let assets = [
        (
            "grade",
            7,
            r#"{"shader":"zshader","stage":"scene_color","order":0}"#,
        ),
        ("jade", 9, SHADER),
        ("panel", 3, r#"{"height":50}"#),
        ("tint", 8, DESCRIPTOR),
        (
            "welcome",
            2,
            r#"{"version":1,"nodes":[{"id":"root","kind":"panel","style":"jade:panel"}]}"#,
        ),
        ("zshader", 6, effect),
    ];
    writer.count(assets.len()).unwrap();
    for (name, kind, bytes) in assets {
        writer.field(name.as_bytes()).unwrap();
        writer.count(kind).unwrap();
        writer.field(bytes.as_bytes()).unwrap();
    }
    writer.count(0).unwrap();
    let bundle = ClientBundle::decode_verify(&writer.0, key(&writer.0)).unwrap();
    assert!(bundle.ui().is_some());
    assert_eq!(bundle.effect().unwrap().owner, "jade:grade");
    assert_eq!(bundle.material().unwrap().materials[0].owner, "jade:tint");
    bundle
        .material()
        .unwrap()
        .resolve(&crate::content::Catalog::builtins())
        .unwrap();
    assert!(bundle.packages()["jade"].textures.is_empty());
}

fn texture_metadata(png: &[u8], texture_key: &str, asset: &str) -> Vec<u8> {
    let mut writer = header(1);
    writer.field(b"jade").unwrap();
    writer.field(b"1.0.0").unwrap();
    writer.count(0).unwrap(); // dependencies
    writer.count(0).unwrap(); // modules
    writer.count(1).unwrap(); // assets
    writer.field(b"tile").unwrap();
    writer.count(1).unwrap();
    writer.field(png).unwrap();
    writer.count(1).unwrap(); // startup present
    writer.count(1).unwrap(); // requirements
    writer.field(b"bloxgloom:content/v1").unwrap();
    writer.count(0).unwrap(); // items
    writer.count(1).unwrap(); // textures
    writer.field(texture_key.as_bytes()).unwrap();
    writer.field(asset.as_bytes()).unwrap();
    writer.count(0).unwrap(); // startup blocks
    for _ in 0..5 {
        writer.count(0).unwrap();
    } // runtime categories
    writer.0
}

#[test]
fn bound_texture_asset_must_decode_and_match_owned_canonical_metadata() {
    let image = include_bytes!(
        "../../../../../../fixtures/material-packages/jade/assets/textures/jade.png"
    );
    let good = texture_metadata(image, "jade:tile", "tile");
    let bundle = ClientBundle::decode_verify(&good, key(&good)).unwrap();
    assert_eq!(
        bundle
            .session_catalog()
            .unwrap()
            .textures()
            .last()
            .unwrap()
            .png
            .as_ref(),
        image
    );
    let replacement = texture_metadata(
        include_bytes!("../../../../../../assets/textures/blocks/hopper_side.png"),
        "jade:tile",
        "tile",
    );
    let other = ClientBundle::decode_verify(&replacement, key(&replacement)).unwrap();
    assert_ne!(bundle.cache_key(), other.cache_key());
    assert_ne!(
        bundle.session_catalog().unwrap().fingerprint(),
        other.session_catalog().unwrap().fingerprint()
    );
    for (image, name, asset) in [
        (&b"not png"[..], "jade:tile", "tile"),
        (&image[..], "foreign:tile", "tile"),
        (&image[..], "jade:tile", "missing"),
    ] {
        let bad = texture_metadata(image, name, asset);
        assert!(ClientBundle::decode_verify(&bad, key(&bad)).is_err());
    }
    let mut huge = Vec::new();
    let mut encoder = png::Encoder::new(&mut huge, 2049, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&vec![0; 2049 * 3])
        .unwrap();
    let bad = texture_metadata(&huge, "jade:tile", "tile");
    assert!(ClientBundle::decode_verify(&bad, key(&bad)).is_err());
}

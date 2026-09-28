use super::*;

const SHADER: &str =
    include_str!("../../../../../../fixtures/material-packages/jade/assets/shaders/jade.wgsl");
const DESCRIPTOR: &str = r#"{"shader":"jade","texture":"bloxgloom:stone"}"#;

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
    assert_eq!(resolved.owner, "jade:tint");
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
    let mut v4 = bytes.clone();
    v4[8] = 4;
    assert!(ClientBundle::decode_verify(&v4, key(&v4)).is_err());
    assert_ne!(verified.cache_key(), key(&v4));
}

#[test]
fn format_two_fixture_discovers_and_resolves_material_without_effect_or_ui() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/material-packages");
    let snapshot = crate::server::PackageSnapshot::discover(&root).unwrap();
    let verified = ClientBundle::decode_verify(
        snapshot.client_bundle().bytes(),
        snapshot.client_bundle().cache_key(),
    )
    .unwrap();
    assert_eq!(verified.material().unwrap().owner, "jade:tint");
    assert!(verified.effect().is_none() && verified.ui().is_none());
    verified
        .material()
        .unwrap()
        .resolve(&crate::content::Catalog::builtins())
        .unwrap();
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
    assert_eq!(bundle.material().unwrap().owner, "jade:tint");
    bundle
        .material()
        .unwrap()
        .resolve(&crate::content::Catalog::builtins())
        .unwrap();
    assert!(bundle.packages()["jade"].textures.is_empty());
}

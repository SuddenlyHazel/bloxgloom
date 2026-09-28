use super::*;

const SHADER: &str =
    include_str!("../../../../../../fixtures/effect-packages/sepia/assets/shaders/sepia.wgsl");
const DESCRIPTOR: &str = r#"{"shader":"shader","stage":"scene_color","order":0}"#;

fn bundle(packages: &[&str], descriptor: &str, shader: &str, reversed: bool) -> Vec<u8> {
    let mut writer = header(packages.len());
    for name in packages {
        writer.field(name.as_bytes()).unwrap();
        writer.field(b"1.0.0").unwrap();
        writer.count(0).unwrap();
        writer.count(0).unwrap();
        writer.count(2).unwrap();
        let mut assets = [("effect", 7, descriptor), ("shader", 6, shader)];
        if reversed {
            assets.reverse();
        }
        for (key, kind, bytes) in assets {
            writer.field(key.as_bytes()).unwrap();
            writer.count(kind).unwrap();
            writer.field(bytes.as_bytes()).unwrap();
        }
    }
    writer.count(0).unwrap();
    writer.0
}

#[test]
fn verified_bundle_prepares_effect_and_rejects_ownership_order_and_shader_failures() {
    let bytes = bundle(&["sepia"], DESCRIPTOR, SHADER, false);
    let decoded = ClientBundle::decode_verify(&bytes, key(&bytes)).unwrap();
    assert_eq!(decoded.effect().unwrap().owner, "sepia:effect");
    assert!(decoded.packages()["sepia"].textures.is_empty());
    assert!(decoded.packages()["sepia"].ui_assets.is_empty());
    for descriptor in [
        DESCRIPTOR.replace("\"order\":0", "\"order\":1"),
        DESCRIPTOR.replace("scene_color", "after_ui"),
        DESCRIPTOR.replace("\"shader\":\"shader\"", "\"shader\":\"foreign:shader\""),
        DESCRIPTOR.replace("\"shader\":\"shader\"", "\"shader\":\"effect\""),
        DESCRIPTOR.replace("{", "{\"extra\":true,"),
        DESCRIPTOR.replace("{", "{\"order\":0,"),
        " ".repeat(1025),
    ] {
        let bad = bundle(&["sepia"], &descriptor, SHADER, false);
        let error = ClientBundle::decode_verify(&bad, key(&bad)).unwrap_err();
        assert!(format!("{error:?}").contains("sepia"));
    }
    for shader in [
        "not wgsl".to_owned(),
        SHADER.replace("fs_main", "other"),
        " ".repeat(16 * 1024 + 1),
    ] {
        let bad = bundle(&["sepia"], DESCRIPTOR, &shader, false);
        let error = ClientBundle::decode_verify(&bad, key(&bad)).unwrap_err();
        assert!(format!("{error:?}").contains("sepia"));
    }
    for bad in [
        bundle(&["a", "b"], DESCRIPTOR, SHADER, false),
        bundle(&["a", "a"], DESCRIPTOR, SHADER, false),
        bundle(&["a"], DESCRIPTOR, SHADER, true),
    ] {
        assert!(ClientBundle::decode_verify(&bad, key(&bad)).is_err());
    }
    let changed = bundle(&["sepia"], DESCRIPTOR, &SHADER.replace("0.8", "0.7"), false);
    assert!(ClientBundle::decode_verify(&changed, decoded.cache_key()).is_err());
}

#[test]
fn format_two_example_discovery_roundtrips_prepared_effect() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/effect-packages");
    let snapshot = crate::server::PackageSnapshot::discover(&root).unwrap();
    crate::server::script::startup::Declarations::discover(&root).unwrap();
    let bundle = snapshot.client_bundle();
    let decoded = ClientBundle::decode_verify(bundle.bytes(), bundle.cache_key()).unwrap();
    assert_eq!(decoded.effect().unwrap().owner, "sepia:grade");
    assert!(decoded.packages()["sepia"].sources.is_empty());
}

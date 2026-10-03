use super::*;
fn asset() -> ModelAsset {
    ModelAsset {
        key: "demo:model".into(),
        glb: include_bytes!("../../../assets/models/player/master/model.glb").to_vec(),
        controls: include_bytes!("../../../assets/models/player/master/controls.json").to_vec(),
        scale: 1.0,
    }
}
#[test]
fn models_share_prepared_data_through_manifest_assignment_and_detect_asset_changes() {
    let asset = asset();
    let mut local = Catalog::builtins();
    local.register_model_asset(&asset).unwrap();
    let expected = local.model_by_key(&asset.key).unwrap();
    assert!(!expected.schema().clips.is_empty());
    let mut manifest = super::super::manifest::ContentManifest::from_catalog(&local);
    let entry = manifest
        .entries
        .iter_mut()
        .find(|e| e.kind == b'M')
        .unwrap();
    entry.id = 17;
    let manifest =
        super::super::manifest::ContentManifest::decode(&manifest.encode().unwrap()).unwrap();
    let mapped = manifest.resolve_catalog(&local).unwrap();
    let (id, actual) = mapped.models().next().unwrap();
    assert_eq!(id, 17);
    assert!(Arc::ptr_eq(expected, actual));
    let mut changed = asset;
    changed.scale = 0.5;
    let mut mismatch = Catalog::builtins();
    mismatch.register_model_asset(&changed).unwrap();
    assert!(manifest.resolve_catalog(&mismatch).is_err());
}
#[test]
fn invalid_model_controls_and_external_assets_fail_closed_without_catalog_changes() {
    let mut asset = asset();
    let mut catalog = Catalog::new();
    for controls in [
        br#"{"layers":[{"name":"missing","nodes":["not_real"],"visible":true}]}"#.as_slice(),
        b"{broken".as_slice(),
    ] {
        asset.controls = controls.to_vec();
        assert!(catalog.register_model_asset(&asset).is_err());
        assert_eq!(catalog.models().count(), 0);
    }
    asset.controls.clear();
    asset.scale = f32::NAN;
    assert!(catalog.register_model_asset(&asset).is_err());
    assert_eq!(catalog.models().count(), 0);
}
#[test]
fn repeated_meshes_are_charged_before_decoding_geometry() {
    let asset = asset();
    let glb = gltf::binary::Glb::from_slice(&asset.glb).unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&glb.json).unwrap();
    let mesh = document["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|n| n.get("mesh").cloned())
        .unwrap();
    let primitives = document["meshes"][mesh.as_u64().unwrap() as usize]["primitives"]
        .as_array()
        .unwrap()
        .clone();
    document["meshes"][mesh.as_u64().unwrap() as usize]["primitives"] =
        serde_json::Value::Array((0..16).flat_map(|_| primitives.clone()).collect());
    document["nodes"] = serde_json::Value::Array(
        (0..1024)
            .map(|_| serde_json::json!({"mesh":mesh}))
            .collect(),
    );
    document["scenes"][0]["nodes"] =
        serde_json::Value::Array((0..1024).map(|n| serde_json::json!(n)).collect());
    let bytes = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: std::borrow::Cow::Owned(serde_json::to_vec(&document).unwrap()),
        bin: glb.bin,
    }
    .to_vec()
    .unwrap();
    assert!(estimate(&bytes).unwrap_err().contains("vertex admission"));
}

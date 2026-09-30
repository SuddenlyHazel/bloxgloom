use super::*;

fn base() -> Vec<u8> {
    let mut writer = Writer(super::super::super::MAGIC.to_vec());
    writer.count(1).unwrap();
    writer.field(b"demo").unwrap();
    writer.field(b"1.0.0").unwrap();
    writer.count(0).unwrap(); // dependencies
    writer.count(0).unwrap(); // client sources
    writer.count(1).unwrap(); // texture asset
    writer.field(b"tile").unwrap();
    writer.count(1).unwrap();
    writer
        .field(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/anchored-counter/packages/counter/assets/textures/counter.png"
        )))
        .unwrap();
    writer.count(1).unwrap(); // startup metadata
    writer.count(2).unwrap();
    writer
        .field(composition::ANCHORED_ENTITIES.as_bytes())
        .unwrap();
    writer.field(composition::CONTENT.as_bytes()).unwrap();
    writer.count(1).unwrap(); // placeable block item
    for field in ["demo:counter_block", "Counter", "demo:tile"] {
        writer.field(field.as_bytes()).unwrap();
    }
    writer.count(1).unwrap(); // textures
    writer.field(b"demo:tile").unwrap();
    writer.field(b"tile").unwrap();
    writer.count(1).unwrap(); // block
    for field in ["demo:counter_block", "Counter", "demo:tile"] {
        writer.field(field.as_bytes()).unwrap();
    }
    writer.field(&[0]).unwrap(); // cube flags
    for _ in 0..5 {
        // runtime categories
        writer.count(0).unwrap();
    }
    writer.0
}

fn declaration() -> AnchoredBlockEntity {
    AnchoredBlockEntity {
        entity: "demo:counter".into(),
        block: "demo:counter_block".into(),
        placement_item: "demo:counter_block".into(),
        anchor_state: "demo:counter_block".into(),
        footprint: vec![
            FootprintCell {
                offset: [0; 3],
                state: "demo:counter_block".into(),
            },
            FootprintCell {
                offset: [16, -16, 16],
                state: "demo:counter_block".into(),
            },
        ],
        placement_cost: 128,
        removal_refund: 127,
        schema_version: 2,
        schema_fingerprint: u64::MAX,
        max_state_bytes: 65_536,
        max_public_bytes: 4096,
        interval: u32::MAX,
        observe: vec![[16, -16, 16]],
        interaction: vec![0, 255, 1],
        behavior: Arc::new(crate::server::script::anchored::ScriptAnchored::client()),
    }
}

fn artifact(d: &AnchoredBlockEntity) -> Vec<u8> {
    artifact_on(&base(), d)
}

fn artifact_on(inner: &[u8], d: &AnchoredBlockEntity) -> Vec<u8> {
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(inner).unwrap();
    writer.count(1).unwrap();
    encode(&mut writer, d).unwrap();
    writer.0
}

fn decode_bytes(bytes: &[u8]) -> Result<ClientBundle, ScriptError> {
    ClientBundle::decode_verify(bytes, CacheKey(Sha256::digest(bytes).into()))
}

#[test]
fn anchored_client_artifact_preserves_full_native_contract_and_catalog_identity() {
    let d = declaration();
    let bytes = artifact(&d);
    let decoded = decode_bytes(&bytes).unwrap();
    let actual = &decoded.declarations.as_ref().unwrap().anchored[0];
    assert_eq!(actual.schema_fingerprint, u64::MAX);
    assert_eq!(actual.footprint, d.footprint);
    assert_eq!(actual.observe, d.observe);
    assert_eq!(actual.interaction, d.interaction);
    assert_eq!(actual.max_state_bytes, 65_536);
    assert_eq!(actual.max_public_bytes, 4096);
    assert_eq!(actual.interval, u32::MAX);
    assert_eq!(actual.placement_cost, 128);
    assert_eq!(actual.removal_refund, 127);
    assert!(actual.behavior.initialize([0; 3]).is_err());
    let mut native = decode_bytes(&base()).unwrap().session_catalog().unwrap();
    native.register_anchored(d).unwrap();
    let client = decoded.session_catalog().unwrap();
    let id = native.entity_type_id_by_key("demo:counter").unwrap();
    assert_eq!(client.entity_type_id_by_key("demo:counter"), Some(id));
    assert_eq!(
        client.entity_type(id).unwrap().schema_fingerprint,
        native.entity_type(id).unwrap().schema_fingerprint
    );
    assert!(client.action("demo:counter/interact").is_some());
}

#[test]
fn anchored_client_artifact_rejects_unresolved_refs_truncation_and_nested_wrappers() {
    let bytes = artifact(&declaration());
    decode_bytes(&bytes).unwrap(); // malformed variants must start from a valid artifact
    for end in 0..bytes.len() {
        assert!(decode_bytes(&bytes[..end]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_bytes(&trailing).is_err());
    let mut builtin = declaration();
    builtin.block = "bloxgloom:stone".into();
    builtin.placement_item = "bloxgloom:stone".into();
    builtin.anchor_state = "bloxgloom:stone".into();
    for cell in &mut builtin.footprint {
        cell.state = "bloxgloom:stone".into();
    }
    assert!(decode_bytes(&artifact(&builtin)).is_err());
    let mut d = declaration();
    d.anchor_state = "bloxgloom:missing".into();
    d.footprint[0].state = d.anchor_state.clone();
    assert!(decode_bytes(&artifact(&d)).is_err());
    let mut nested = Writer(MAGIC.to_vec());
    nested.field(&bytes).unwrap();
    nested.count(1).unwrap();
    encode(&mut nested, &declaration()).unwrap();
    assert!(decode_bytes(&nested.0).is_err());
}

#[test]
fn anchored_client_artifact_rejects_storage_and_machine_ownership_collisions() {
    for kind in ["storage", "machine"] {
        let directory = std::env::temp_dir().join(format!(
            "bloxgloom-anchored-codec-{}-{}-{}",
            std::process::id(),
            kind,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let package = directory.join("demo");
        std::fs::create_dir_all(package.join("server")).unwrap();
        std::fs::create_dir_all(package.join("assets/textures")).unwrap();
        std::fs::write(
            package.join("assets/textures/tile.png"),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/fixtures/anchored-counter/packages/counter/assets/textures/counter.png"
            )),
        )
        .unwrap();
        std::fs::write(package.join("package.txt"), concat!(
            "format 2\npackage demo\nversion 1.0.0\nentry main\n",
            "module server main server/main.luau\nmodule server tick server/tick.luau\n",
            "asset texture tile assets/textures/tile.png\n",
            "requires bloxgloom:content/v1\nrequires bloxgloom:anchored_entities/v1\n",
            "requires bloxgloom:inventory_screens/v1\nrequires bloxgloom:storage/v1\nrequires bloxgloom:machines/v1\n"
        )).unwrap();
        let registration = if kind == "storage" {
            "h.register_storage('demo:owner','demo:counter_block','Counter',1,1)"
        } else {
            "h.register_machine{entity='demo:owner',block='demo:counter_block',module='demo:tick',schema=1,revision=1,interval=20,title='Counter',recipe={key='demo:crush',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=1,pulses=1}}"
        };
        std::fs::write(package.join("server/main.luau"), format!("return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:counter_block','Counter','demo:tile'); {registration} end")).unwrap();
        std::fs::write(
            package.join("server/tick.luau"),
            "return function(e) return '',1 end",
        )
        .unwrap();
        // macOS temp paths contain /var symlinks; discovery deliberately refuses
        // symlinks in every directory component. Resolve our own test root first.
        let root = std::fs::canonicalize(&directory).unwrap();
        let startup = crate::server::script::startup::Declarations::discover(&root).unwrap();
        let bytes = startup.client_bundle.bytes();
        decode_bytes(bytes).unwrap();
        assert!(
            decode_bytes(&artifact_on(bytes, &declaration())).is_err(),
            "{kind} ownership collision accepted"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}

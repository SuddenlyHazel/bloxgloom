use super::*;
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey, STONE};
use std::time::Duration;

const REGISTER: &str = "return function(h) h.register_texture('demo:tile','tile'); h.register_item('demo:token','Token','demo:tile'); h.register_block('demo:jade','Jade','demo:tile') end";

fn package(fixture: &Fixture, source: &str) {
    let dir = fixture.0.join("packages/demo");
    std::fs::create_dir_all(dir.join("server")).unwrap();
    std::fs::create_dir_all(dir.join("assets/textures")).unwrap();
    std::fs::write(dir.join("server/main.luau"), source).unwrap();
    std::fs::write(
        dir.join("package.txt"),
        concat!(
            "format 2\npackage demo\nversion 1.0.0\nentry main\n",
            "requires bloxgloom:content/v1\nmodule server main server/main.luau\n",
            "asset texture tile assets/textures/jade.png\n"
        ),
    )
    .unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/material-packages/jade/assets/textures/jade.png"
        ),
        dir.join("assets/textures/jade.png"),
    )
    .unwrap();
}

fn connect(address: std::net::SocketAddr, catalog: Arc<Catalog>, profile: u128) -> Peer {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    protocol::write_client(
        &mut stream,
        &ClientMessage::Hello {
            name: "cube".into(),
            profile,
            content_fingerprint: catalog.fingerprint(),
        },
    )
    .unwrap();
    let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut stream).unwrap()
    else {
        panic!("expected bundle offer");
    };
    crate::client::bundle::receive(&mut stream, identity, None).unwrap();
    let (fingerprint, bytes) = receive_content_manifest(&mut stream);
    assert_eq!(fingerprint, catalog.fingerprint());
    assert_eq!(
        bytes,
        crate::content::ContentManifest::from_catalog(&catalog)
            .encode()
            .unwrap()
    );
    protocol::write_client(&mut stream, &ClientMessage::ContentReady { fingerprint }).unwrap();
    let mut peer = Peer {
        stream,
        catalog,
        epoch: 0,
        sequence: 0,
        inventory: Inventory::default(),
    };
    let mut session = false;
    let mut inventory = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !session || !inventory {
        match peer.read(deadline) {
            ServerMessage::ActionSession {
                epoch, next_seq, ..
            } => {
                peer.epoch = epoch;
                peer.sequence = next_seq;
                session = true;
            }
            ServerMessage::Inventory { .. } => inventory = true,
            _ => {}
        }
    }
    peer
}

#[test]
fn package_cube_joins_places_and_recovers_with_identical_session_catalog() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    package(&fixture, REGISTER);
    let dir = fixture.0.join("packages/demo");
    let sample =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/material-packages/jade");
    for path in ["assets/materials/tint.json", "assets/shaders/jade.wgsl"] {
        std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
        std::fs::copy(sample.join(path), dir.join(path)).unwrap();
    }
    std::fs::write(
        dir.join("assets/materials/tint.json"),
        r#"{"shader":"jade","texture":"demo:tile"}"#,
    )
    .unwrap();
    let mut manifest = std::fs::read_to_string(dir.join("package.txt")).unwrap();
    manifest.push_str("asset material tint assets/materials/tint.json\nasset material-shader jade assets/shaders/jade.wgsl\n");
    std::fs::write(dir.join("package.txt"), manifest).unwrap();
    let mut identity = None;
    for round in 0..2 {
        if round == 1 {
            std::fs::write(dir.join("server/main.luau"),
                "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile'); h.register_item('demo:token','Token','demo:tile') end"
            ).unwrap();
        }
        let mut state = Box::new(fixture.open().unwrap());
        state.spawn_anchor = [0.5, 80.0, 0.5];
        state.world.edit(0, 79, 0, STONE).unwrap();
        for y in [80, 81] {
            state.world.edit(0, y, 0, AIR).unwrap();
        }
        if round == 0 {
            state.world.edit(2, 80, 0, AIR).unwrap();
        }
        let catalog = state.world.catalog_arc();
        let block = catalog.state_by_key("demo:jade").unwrap();
        let item = catalog.item_by_key("demo:jade").unwrap();
        assert_eq!(catalog.item(item).unwrap().placeable, Some(block));
        let texture = catalog
            .texture(catalog.block(block).unwrap().textures.side)
            .unwrap();
        assert_eq!(texture.key.as_ref(), "demo:tile");
        assert_eq!(
            texture.png.as_ref(),
            std::fs::read(dir.join("assets/textures/jade.png")).unwrap()
        );
        let manifest = crate::content::ContentManifest::from_catalog(&catalog)
            .encode()
            .unwrap();
        let current = (block, item, catalog.fingerprint(), manifest);
        if let Some(prior) = &identity {
            assert_eq!(prior, &current);
        } else {
            identity = Some(current);
        }
        if round == 0 {
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::new(item, 2));
            state.inventory_store.save(PROFILE, &inventory).unwrap();
            // Inventory and position checkpoints have independent temporary-file
            // counters in the same directory; do not race identical temp names.
            state
                .position_store
                .save(PROFILE, [0.5, 80.0, 0.5])
                .unwrap();
        } else {
            assert_eq!(state.world.get_block(2, 80, 0).unwrap(), block);
        }
        serve(state, |address| {
            let bundle = crate::client::connect_bundle_probe(&address.to_string(), 0xabc0 + round)
                .unwrap()
                .unwrap();
            let joined = bundle.session_catalog().unwrap();
            assert_eq!(joined.fingerprint(), catalog.fingerprint());
            assert_eq!(
                joined
                    .texture(joined.block(block).unwrap().textures.side)
                    .unwrap()
                    .png,
                catalog
                    .texture(catalog.block(block).unwrap().textures.side)
                    .unwrap()
                    .png
            );
            assert_eq!(
                crate::content::ContentManifest::from_catalog(&joined)
                    .encode()
                    .unwrap(),
                crate::content::ContentManifest::from_catalog(&catalog)
                    .encode()
                    .unwrap()
            );
            let material = bundle.material().unwrap().resolve(&joined).unwrap();
            let layer = joined.block(block).unwrap().textures.side.get() as f32;
            assert_eq!(material.owner, "demo:tint");
            assert_eq!(material.selected_layer() as f32, layer);
            // World geometry, not an item sprite, carries the shader-selected layer.
            let mut chunk = Chunk {
                key: ChunkKey { x: 0, y: 10, z: 0 },
                version: 1,
                blocks: vec![AIR; CHUNK_SIZE.pow(3)].into(),
            };
            chunk.blocks.set(Chunk::index([2, 0, 0]).unwrap(), block);
            let known = std::collections::HashMap::from([(chunk.key, Arc::new(chunk.clone()))]);
            let light =
                crate::lighting::LightField::build_with_catalog(chunk.key, &known, 1, &joined);
            let mesh = crate::render::mesh_chunk_lit_with_catalog(&chunk, &light, 1, &joined);
            assert_eq!(mesh.indices.len(), 36);
            assert!(
                mesh.vertices
                    .chunks_exact(crate::render::VERTEX_FLOATS)
                    .all(|v| v[8] == layer)
            );
            if round == 0 {
                let mut peer = connect(address, Arc::clone(&catalog), PROFILE);
                let action_id = (u128::from(peer.epoch) << 64) | u128::from(peer.sequence);
                let result = peer.send(&ClientMessage::Edit {
                    action_id,
                    x: 2,
                    y: 80,
                    z: 0,
                    block,
                    slot: 0,
                });
                assert!(result.0, "{}", result.1);
                peer.inventory_at(1);
            }
        });
    }
}

#[test]
fn package_cube_flags_are_frozen_and_old_declaration_keeps_defaults() {
    let source = |options: &str| {
        format!(
            "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile'{options}) end"
        )
    };
    let defaults = Fixture::new();
    package(&defaults, &source(""));
    let default_state = defaults.open().unwrap();
    let default_catalog = default_state.world.catalog();
    let default_block = default_catalog.state_by_key("demo:jade").unwrap();
    assert_eq!(
        default_catalog.block_flags(default_block)
            & (crate::content::FLAMMABLE | crate::content::SUPPORTS_PLANT),
        0
    );

    let configured = Fixture::new();
    package(
        &configured,
        &source(", {flammable=true, supports_plant=true}"),
    );
    let state = Box::new(configured.open().unwrap());
    let catalog = state.world.catalog_arc();
    let block = catalog.state_by_key("demo:jade").unwrap();
    assert_eq!(
        catalog.block_flags(block) & (crate::content::FLAMMABLE | crate::content::SUPPORTS_PLANT),
        crate::content::FLAMMABLE | crate::content::SUPPORTS_PLANT
    );
    assert_ne!(catalog.fingerprint(), default_catalog.fingerprint());
    let fingerprint = catalog.fingerprint();
    serve(state, |address| {
        let bundle = crate::client::connect_bundle_probe(&address.to_string(), 0xabc1)
            .unwrap()
            .unwrap();
        let joined = bundle.session_catalog().unwrap();
        assert_eq!(joined.fingerprint(), fingerprint);
        assert_eq!(joined.block_flags(block), catalog.block_flags(block));
    });
    assert_eq!(
        fingerprint,
        configured.open().unwrap().world.catalog().fingerprint()
    );
}

#[test]
fn package_cube_material_options_negotiate_and_survive_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let default = Fixture::new();
    package(
        &default,
        "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile') end",
    );
    let default_bundle = default.startup(Arc::new(Catalog::builtins())).unwrap();
    assert!(
        default_bundle
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x07")
    );
    let explicit = Fixture::new();
    package(
        &explicit,
        "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile',{solid=true,replaceable=false,emission=0,reflectance={128,128,128}}) end",
    );
    let explicit_bundle = explicit.startup(Arc::new(Catalog::builtins())).unwrap();
    assert_eq!(
        default_bundle.client_bundle.as_ref().unwrap().cache_key(),
        explicit_bundle.client_bundle.as_ref().unwrap().cache_key()
    );
    let fixture = Fixture::new();
    package(
        &fixture,
        "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile',{solid=false,replaceable=true,emission=8,reflectance={12,34,56},side='demo:tile',bottom='demo:tile'}) end",
    );
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let jade = catalog.state_by_key("demo:jade").unwrap();
    let block = catalog.block(jade).unwrap();
    assert!(!block.solid);
    assert!(block.replaceable);
    assert_eq!(block.emission, 8);
    assert_eq!(block.reflectance, [12, 34, 56]);
    let fingerprint = catalog.fingerprint();
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x17")
    );
    serve(state, |address| {
        let joined = crate::client::connect_catalog_probe(&address.to_string(), 0xabc2).unwrap();
        assert_eq!(joined.fingerprint(), fingerprint);
        assert_eq!(joined.block(jade).unwrap().reflectance, [12, 34, 56]);
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    let saved_map = std::fs::read(fixture.0.join("save/content.map")).unwrap();
    package(
        &fixture,
        "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile',{solid=false,replaceable=true,emission=9,reflectance={12,34,56}}) end",
    );
    assert!(fixture.open().is_err());
    assert_eq!(
        std::fs::read(fixture.0.join("save/content.map")).unwrap(),
        saved_map
    );
}

#[test]
fn package_cube_rejections_fail_before_world_open() {
    for (source, expected) in [
        (
            "h.register_block('foreign:jade','Jade','demo:tile')",
            "namespace",
        ),
        (
            "h.register_block('demo:jade','Jade','foreign:tile')",
            "package-owned",
        ),
        (
            "h.register_block('demo:jade','Jade','demo:missing')",
            "registered package texture",
        ),
        (
            "h.register_block('demo:jade','Jade','demo:tile'); h.register_block('demo:jade','Jade','demo:tile')",
            "duplicate",
        ),
        (
            "h.register_item('demo:jade','Jade','demo:tile'); h.register_block('demo:jade','Jade','demo:tile')",
            "duplicate",
        ),
        (
            "pcall(function() h.register_block('foreign:jade','Jade','demo:tile') end)",
            "namespace",
        ),
        (
            "h.register_block('demo:jade',string.rep('x',256),'demo:tile')",
            "255 bytes",
        ),
        (
            "h.register_block('demo:jade','Jade','demo:tile',{flammable=1})",
            "boolean",
        ),
        (
            "h.register_block('demo:jade','Jade','demo:tile',{madeup=true})",
            "unknown block option",
        ),
        (
            "h.register_block('demo:jade','Jade','demo:tile',{side='demo:missing'})",
            "registered package-owned textures",
        ),
        (
            "h.register_block('demo:jade','Jade','demo:tile',{emission=16})",
            "integer out of bounds",
        ),
        (
            "h.register_block('demo:jade','Jade','demo:tile',{reflectance={1,2,300}})",
            "integer out of bounds",
        ),
        (
            "pcall(function() h.register_block('demo:jade','Jade','demo:tile',{supports_plant='yes'}) end)",
            "boolean",
        ),
        (
            "for i=1,33 do h.register_block('demo:jade'..i,'Jade','demo:tile') end",
            "limit exceeded",
        ),
    ] {
        let fixture = Fixture::new();
        package(
            &fixture,
            &format!("return function(h) h.register_texture('demo:tile','tile'); {source} end"),
        );
        let error = fixture
            .open()
            .err()
            .expect("invalid cube must not open world");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(!fixture.0.join("save").exists());
    }
    let fixture = Fixture::new();
    package(
        &fixture,
        "return function(h) h.register_block('demo:jade','Jade','demo:tile') end",
    );
    std::fs::write(fixture.0.join("packages/demo/package.txt"),
        "format 2\npackage demo\nversion 1.0.0\nentry main\nmodule server main server/main.luau\nasset texture tile assets/textures/jade.png\n"
    ).unwrap();
    let error = fixture
        .open()
        .err()
        .expect("missing capability must refuse startup");
    assert!(error.to_string().contains("content/v1"), "{error}");
    assert!(!fixture.0.join("save").exists());
}

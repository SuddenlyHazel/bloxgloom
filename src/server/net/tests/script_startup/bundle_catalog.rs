//! Full Network preparation against the real nonblocking reactor.
use super::*;
use crate::content::ContentManifest;
use crate::server::client_bundle::{CacheKey, ClientBundle};
use std::net::Shutdown;

#[test]
fn authored_drop_animation_negotiates_and_default_keeps_old_bundle() {
    use bloxgloom_host_api::content::DropAnimation;
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let entry = |options: &str| {
        format!(
            "return function(h) h.register_item('demo:token', 'Token', 'bloxgloom:stone'{options}) end"
        )
    };
    fixture.package("demo", CONTENT, &entry(""));
    let base = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    let key = base.client_bundle.as_ref().unwrap().cache_key();
    let fingerprint = base.catalog().fingerprint();
    fixture.package(
        "demo",
        CONTENT,
        &entry(", { drop_animation = { pickup_duration = 0.34 } }"),
    );
    let same = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    assert_eq!(same.client_bundle.as_ref().unwrap().cache_key(), key);
    assert_eq!(same.catalog().fingerprint(), fingerprint);
    fixture.package(
        "demo",
        CONTENT,
        &entry(", { drop_animation = { pickup_duration = 0.8, pickup_arc = 1.2 } }"),
    );
    let state = Box::new(fixture.open().unwrap());
    let server = state.world.catalog_arc();
    assert_ne!(server.fingerprint(), fingerprint);
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x09")
    );
    let manifest = ContentManifest::from_catalog(&server);
    gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x70a).unwrap();
        assert_eq!(ContentManifest::from_catalog(&client), manifest);
        assert_eq!(
            client.drop_animation(client.item_by_key("demo:token").unwrap()),
            DropAnimation {
                pickup_duration: 0.8,
                pickup_arc: 1.2,
                ..Default::default()
            }
        );
    });
}

#[test]
fn drop_size_option_negotiates_verified_catalog_and_explicit_normal_preserves_bundle() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let entry = |option: &str| {
        format!(
            "return function(h) h.register_item('demo:token', 'Token', 'bloxgloom:stone'{option}) end"
        )
    };
    fixture.package("demo", CONTENT, &entry(""));
    let default = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    let default_key = default.client_bundle.as_ref().unwrap().cache_key();
    let default_identity = default.catalog().fingerprint();
    assert!(
        default
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x07")
    );
    fixture.package("demo", CONTENT, &entry(", { drop_size = 'normal' }"));
    let normal = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    assert_eq!(
        normal.client_bundle.as_ref().unwrap().cache_key(),
        default_key
    );
    assert_eq!(normal.catalog().fingerprint(), default_identity);

    fixture.package("demo", CONTENT, &entry(", { drop_size = 'large' }"));
    let state = Box::new(fixture.open().unwrap());
    let server = state.world.catalog_arc();
    assert_ne!(server.fingerprint(), default_identity);
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x08")
    );
    let expected = ContentManifest::from_catalog(&server);
    gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x709).unwrap();
        assert_eq!(ContentManifest::from_catalog(&client), expected);
        assert_eq!(
            client.drop_size(client.item_by_key("demo:token").unwrap()),
            bloxgloom_host_api::content::DropSize::Large
        );
    });
}

#[test]
fn item_sprite_option_is_negotiated_and_omission_preserves_default_identity() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let entry = |options: &str| {
        format!(
            "return function(h) h.register_item('demo:token', 'Token', 'bloxgloom:stone'{options}) end"
        )
    };
    fixture.package("demo", CONTENT, &entry(""));
    let default = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    let default_key = default.client_bundle.as_ref().unwrap().cache_key();
    let default_catalog = default.catalog();
    let default_fingerprint = default_catalog.fingerprint();
    assert!(
        default_catalog
            .item_by_key("demo:token")
            .is_some_and(|id| default_catalog.item(id).unwrap().sprite)
    );

    fixture.package("demo", CONTENT, &entry(", { sprite = true }"));
    let explicit = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    assert_eq!(
        explicit.client_bundle.as_ref().unwrap().cache_key(),
        default_key
    );
    assert_eq!(explicit.catalog().fingerprint(), default_fingerprint);

    fixture.package("demo", CONTENT, &entry(", { sprite = false }"));
    let state = Box::new(fixture.open().unwrap());
    let server = state.world.catalog_arc();
    assert_ne!(
        state.client_bundle.as_ref().unwrap().cache_key(),
        default_key
    );
    assert_ne!(server.fingerprint(), default_fingerprint);
    let expected_manifest = ContentManifest::from_catalog(&server);
    gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x705).unwrap();
        assert_eq!(client.fingerprint(), server.fingerprint());
        assert_eq!(ContentManifest::from_catalog(&client), expected_manifest);
        let item = client
            .item(client.item_by_key("demo:token").unwrap())
            .unwrap();
        assert!(!item.sprite);
        assert!(item.placeable.is_none());
    });
}

#[test]
fn sprite_catalog_restart_remaps_saved_ids_and_switches_without_global_state() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let global = crate::content::catalog().fingerprint();
    let builtin = Catalog::builtins();
    let mut token_id = None;
    let mut previous_key = None;
    for round in 0..3 {
        // The second startup adds a lexically earlier item. Fresh declaration
        // IDs differ from saved IDs, so the client MUST resolve the manifest.
        let extra = if round > 0 {
            "h.register_item('demo:aaa', 'Earlier', 'bloxgloom:dirt');"
        } else {
            ""
        };
        fixture.package("helper", "", "return function(_) end");
        fixture.package("demo", &format!("{CONTENT}\ndependency helper 1.0.0"),
            &format!("return function(h) {extra} h.register_item('demo:token', 'Token', 'bloxgloom:stone') end"));
        let state = Box::new(fixture.open().unwrap());
        let expected = state.world.catalog_arc();
        let expected_item = expected.items().find(|i| i.key == "demo:token").unwrap().id;
        if round == 0 {
            let mut inventory = Inventory::default();
            assert_eq!(
                inventory.insert_with_catalog(expected_item, 128, &expected),
                0
            );
            state.inventory_store.save(0x701, &inventory).unwrap();
        }
        let expected_manifest = ContentManifest::from_catalog(&expected);
        let key = state.client_bundle.as_ref().unwrap().cache_key();
        if let Some(previous) = previous_key {
            assert_eq!(key == previous, round == 2);
        }
        previous_key = Some(key);
        gameplay::serve(state, |address| {
            let catalog =
                crate::client::connect_inventory_probe(&address.to_string(), 0x701, expected_item)
                    .unwrap();
            assert_eq!(catalog.fingerprint(), expected.fingerprint());
            assert_eq!(ContentManifest::from_catalog(&catalog), expected_manifest);
            let item = catalog.items().find(|i| i.key == "demo:token").unwrap();
            assert!(item.sprite && item.placeable.is_none());
            assert_eq!(item.swatch, [1.0; 4]);
            if let Some(prior) = token_id {
                assert_eq!(item.id, prior);
            }
            token_id = Some(item.id);
            for item in builtin.items() {
                assert_eq!(catalog.item(item.id).unwrap().key, item.key);
            }
            assert_eq!(crate::content::catalog().fingerprint(), global);
        });
    }

    let plain = Fixture::new();
    let state = crate::server::server_state_with_startup(
        7,
        plain.0.join("save"),
        2,
        ServerStartup::new(Arc::new(builtin.clone())),
    )
    .unwrap();
    gameplay::serve(Box::new(state), |address| {
        let catalog = crate::client::connect_catalog_probe(&address.to_string(), 0x702).unwrap();
        assert_eq!(
            ContentManifest::from_catalog(&catalog),
            ContentManifest::from_catalog(&builtin)
        );
        assert!(catalog.item(token_id.unwrap()).is_none());
        assert_eq!(crate::content::catalog().fingerprint(), global);
    });
    // Switching back also rebuilds a fresh matching catalog, including a cache hit.
    let state = Box::new(fixture.open().unwrap());
    let expected = state.world.catalog_arc().fingerprint();
    gameplay::serve(state, |address| {
        assert_eq!(
            crate::client::connect_catalog_probe(&address.to_string(), 0x703)
                .unwrap()
                .fingerprint(),
            expected
        );
    });
}

#[test]
fn production_network_rejects_tampered_metadata_and_exact_manifest_mismatch() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    use sha2::{Digest, Sha256};
    let fixture = Fixture::new();
    super::bundle_runtime::packages(&fixture);
    fixture.package(
        "rules",
        CONTENT,
        &super::player::source("rules:small", 1, super::player::FIELDS),
    );
    gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
        for mode in 0..9 {
            // The upstream is the real server; the relay changes either artifact
            // metadata (with/without updating the offered digest) or fingerprint.
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let relay_address = listener.local_addr().unwrap();
            let worker = thread::spawn(move || {
                let (mut downstream, _) = listener.accept().unwrap();
                downstream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                downstream
                    .set_write_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut upstream = TcpStream::connect(address).unwrap();
                upstream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                upstream
                    .set_write_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let hello = protocol::read_client(&mut downstream).unwrap();
                protocol::write_client(&mut upstream, &hello).unwrap();
                let ServerMessage::BundleOffer { mut identity } =
                    protocol::read_server(&mut upstream).unwrap()
                else {
                    panic!("offer")
                };
                let original = identity;
                protocol::write_client(&mut upstream, &ClientMessage::BundleRequest { identity })
                    .unwrap();
                let mut bytes = Vec::new();
                while bytes.len() < identity.total_len as usize {
                    let ServerMessage::BundlePart {
                        offset,
                        bytes: part,
                    } = protocol::read_server(&mut upstream).unwrap()
                    else {
                        panic!("part")
                    };
                    assert_eq!(offset as usize, bytes.len());
                    bytes.extend_from_slice(&part);
                }
                // Token occurs only in inert metadata: format-1 source is server-only.
                if mode < 2 {
                    let offset = bytes.windows(5).position(|w| w == b"Token").unwrap();
                    bytes[offset] = b'B';
                }
                if mode == 3 {
                    let offset = bytes.windows(9).position(|w| w == b"Use Token").unwrap();
                    bytes[offset] = b'X';
                }
                if matches!(mode, 4 | 5) {
                    let target = if mode == 4 {
                        b"demo:tick".as_slice()
                    } else {
                        b"demo:clock".as_slice()
                    };
                    let offset = bytes
                        .windows(target.len())
                        .position(|w| w == target)
                        .unwrap()
                        + target.len();
                    bytes[offset] ^= 1; // handler/owner compatibility fingerprint
                }
                if mode == 6 {
                    let target = b"demo:counter";
                    let offset = bytes
                        .windows(target.len())
                        .position(|w| w == target)
                        .unwrap()
                        + target.len()
                        + 4
                        + 8;
                    bytes[offset] ^= 1; // entity max_state_bytes is catalog identity too
                }
                if mode == 7 {
                    let offset = bytes.len() - 40 + 20;
                    bytes[offset..offset + 4].copy_from_slice(&0.6f32.to_le_bytes());
                }
                if mode == 8 {
                    let offset = bytes.len() - 40;
                    bytes[offset..offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
                }
                if mode == 1 || mode >= 3 {
                    identity.key = CacheKey::from_bytes(Sha256::digest(&bytes).into());
                    if mode == 8 {
                        assert!(ClientBundle::decode_verify(&bytes, identity.key).is_err());
                    } else {
                        ClientBundle::decode_verify(&bytes, identity.key).unwrap();
                    }
                }
                protocol::write_server(&mut downstream, &ServerMessage::BundleOffer { identity })
                    .unwrap();
                match protocol::read_client(&mut downstream).unwrap() {
                    ClientMessage::BundleRequest {
                        identity: requested,
                    } => {
                        assert_eq!(requested, identity);
                        protocol::write_server(
                            &mut downstream,
                            &ServerMessage::BundlePart { offset: 0, bytes },
                        )
                        .unwrap();
                        if mode == 0 || mode == 8 {
                            assert!(
                                protocol::read_client(&mut downstream).is_err(),
                                "tampered bytes acknowledged"
                            );
                            return;
                        }
                        assert_eq!(
                            protocol::read_client(&mut downstream).unwrap(),
                            ClientMessage::BundleReady { identity }
                        );
                    }
                    ClientMessage::BundleReady { identity: ready } if mode == 2 => {
                        assert_eq!(ready, identity)
                    }
                    other => panic!("unexpected preparation message {other:?}"),
                }
                protocol::write_client(
                    &mut upstream,
                    &ClientMessage::BundleReady { identity: original },
                )
                .unwrap();
                loop {
                    let ServerMessage::ContentManifestPart {
                        mut fingerprint,
                        total_len,
                        offset,
                        bytes,
                    } = protocol::read_server(&mut upstream).unwrap()
                    else {
                        panic!("manifest")
                    };
                    let last = offset as usize + bytes.len() == total_len as usize;
                    if mode == 2 {
                        fingerprint ^= 1;
                    }
                    protocol::write_server(
                        &mut downstream,
                        &ServerMessage::ContentManifestPart {
                            fingerprint,
                            total_len,
                            offset,
                            bytes,
                        },
                    )
                    .unwrap();
                    if last {
                        break;
                    }
                }
                assert!(
                    protocol::read_client(&mut downstream).is_err(),
                    "mismatch must not send ContentReady"
                );
                let _ = upstream.shutdown(Shutdown::Both);
            });
            let error =
                crate::client::connect_catalog_probe(&relay_address.to_string(), 0x710 + mode)
                    .unwrap_err();
            if mode == 0 {
                assert!(error.to_string().contains("SHA-256 integrity mismatch"));
            } else if mode == 8 {
                assert!(error.to_string().contains("invalid"), "{error}");
            } else if mode == 1 || mode >= 3 {
                assert!(error.to_string().contains("schema or material differs"));
            } else {
                assert!(error.to_string().contains("fingerprint mismatch"));
            }
            worker.join().unwrap();
        }
        // Failed preparation did not occupy either player slot.
        crate::client::connect_catalog_probe(&address.to_string(), 0x710).unwrap();
        crate::client::connect_catalog_probe(&address.to_string(), 0x711).unwrap();
    });
}

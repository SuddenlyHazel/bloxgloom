//! Real reactor joins, including hostile handshakes and a corrupting relay.
use super::*;

#[path = "bundle/progress.rs"]
mod progress;
use crate::protocol::{BundleIdentity, MAX_BUNDLE_PART};
use crate::server::client_bundle::{CacheKey, ClientBundle, MAX_BUNDLE_BYTES};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr};

fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.package("demo", "", "return function(_) end");
    let dir = fixture.0.join("packages/demo");
    for path in ["server", "client", "assets/textures"] {
        std::fs::create_dir_all(dir.join(path)).unwrap();
    }
    std::fs::write(dir.join("server/main.luau"), "return function(_) end").unwrap();
    std::fs::write(
        dir.join("package.txt"),
        concat!(
            "format 2\npackage demo\nversion 1.0.0\nentry main\n",
            "module server main server/main.luau\nmodule client view client/view.luau\n",
            "asset texture icon assets/textures/icon.png\n"
        ),
    )
    .unwrap();
    // Never executed or decoded as an image by this preparation increment.
    std::fs::write(dir.join("client/view.luau"), "error('must not execute')").unwrap();
    std::fs::write(
        dir.join("assets/textures/icon.png"),
        vec![0x5a; 3 * MAX_BUNDLE_PART],
    )
    .unwrap();
    fixture
}

#[test]
fn package_effect_is_prepared_by_real_client_join_before_welcome() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = fixture();
    let dir = fixture.0.join("packages/demo");
    let sample =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/effect-packages/sepia");
    for path in ["assets/shaders/sepia.wgsl", "assets/effects/grade.json"] {
        std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
        std::fs::copy(sample.join(path), dir.join(path)).unwrap();
    }
    let manifest = dir.join("package.txt");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("asset shader sepia assets/shaders/sepia.wgsl\nasset effect grade assets/effects/grade.json\n");
    std::fs::write(manifest, text).unwrap();
    gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
        let bundle = crate::client::connect_bundle_probe(&address.to_string(), 0xeffec7)
            .unwrap()
            .unwrap();
        assert_eq!(bundle.effect().unwrap().owner, "demo:grade");
    });
}

#[test]
fn package_material_is_verified_and_resolved_by_real_client_join_before_welcome() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = fixture();
    let dir = fixture.0.join("packages/demo");
    let sample =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/material-packages/jade");
    for path in [
        "assets/materials/tint.json",
        "assets/shaders/jade.wgsl",
        "assets/textures/jade.png",
    ] {
        std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
        std::fs::copy(sample.join(path), dir.join(path)).unwrap();
    }
    std::fs::write(
        dir.join("assets/materials/tint.json"),
        r#"{"shader":"jade","texture":"demo:tile"}"#,
    )
    .unwrap();
    std::fs::write(dir.join("server/main.luau"),
        "return function(host) host.register_texture('demo:tile','tile'); host.register_item('demo:token','Demo Token','demo:tile') end").unwrap();
    let effect =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/effect-packages/sepia");
    for path in ["assets/shaders/sepia.wgsl", "assets/effects/grade.json"] {
        std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
        std::fs::copy(effect.join(path), dir.join(path)).unwrap();
    }
    let manifest = dir.join("package.txt");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("requires bloxgloom:content/v1\nasset texture tile assets/textures/jade.png\nasset material tint assets/materials/tint.json\nasset material-shader jade assets/shaders/jade.wgsl\nasset shader sepia assets/shaders/sepia.wgsl\nasset effect grade assets/effects/grade.json\n");
    std::fs::write(manifest, text).unwrap();
    let state = Box::new(fixture.open().unwrap());
    let server_catalog = state.world.catalog_arc();
    gameplay::serve(state, |address| {
        // The probe uses the real nonblocking reactor and Network::connect;
        // material resolution happens before ContentReady and Welcome.
        let bundle = crate::client::connect_bundle_probe(&address.to_string(), 0x7ade)
            .unwrap()
            .unwrap();
        assert_eq!(bundle.material().unwrap().materials[0].owner, "demo:tint");
        assert_eq!(bundle.effect().unwrap().owner, "demo:grade");
        let catalog = bundle.session_catalog().unwrap();
        let item = catalog.item_by_key("demo:token").unwrap();
        assert_eq!(
            catalog
                .texture(catalog.item(item).unwrap().texture)
                .unwrap()
                .key
                .as_ref(),
            "demo:tile"
        );
        let joined = crate::client::connect_catalog_probe(&address.to_string(), 0x7ae0).unwrap();
        assert_eq!(joined.fingerprint(), server_catalog.fingerprint());
        let joined_item = joined.item_by_key("demo:token").unwrap();
        assert_eq!(
            joined
                .texture(joined.item(joined_item).unwrap().texture)
                .unwrap()
                .png
                .as_ref(),
            server_catalog
                .texture(
                    server_catalog
                        .item(server_catalog.item_by_key("demo:token").unwrap())
                        .unwrap()
                        .texture
                )
                .unwrap()
                .png
                .as_ref()
        );
    });
}

#[test]
fn unresolved_material_texture_refuses_client_join_with_package_identity() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = fixture();
    let dir = fixture.0.join("packages/demo");
    let sample =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/material-packages/jade");
    for path in ["assets/materials/tint.json", "assets/shaders/jade.wgsl"] {
        std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
        std::fs::copy(sample.join(path), dir.join(path)).unwrap();
    }
    std::fs::write(
        dir.join("assets/materials/tint.json"),
        r#"{"shader":"jade","texture":"demo:missing"}"#,
    )
    .unwrap();
    let manifest = dir.join("package.txt");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("asset material tint assets/materials/tint.json\nasset material-shader jade assets/shaders/jade.wgsl\n");
    std::fs::write(manifest, text).unwrap();
    gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
        let error = crate::client::connect_bundle_probe(&address.to_string(), 0x7adf).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("demo:tint"), "{error}");
    });
}

#[test]
fn startup_texture_binding_rejects_missing_capability_foreign_keys_and_bad_png() {
    let image =
        include_bytes!("../../../../../fixtures/material-packages/jade/assets/textures/jade.png");
    for (requires, source, png, expected) in [
        (
            "",
            "host.register_texture('demo:tile','tile')",
            &image[..],
            "content/v1",
        ),
        (
            "requires bloxgloom:content/v1\n",
            "host.register_texture('foreign:tile','tile')",
            &image[..],
            "namespace",
        ),
        (
            "requires bloxgloom:content/v1\n",
            "host.register_texture('demo:tile','missing')",
            &image[..],
            "declared PNG",
        ),
        (
            "requires bloxgloom:content/v1\n",
            "host.register_texture('demo:tile','tile')",
            &b"invalid png"[..],
            "InvalidTexture",
        ),
    ] {
        let fixture = fixture();
        let dir = fixture.0.join("packages/demo");
        std::fs::write(dir.join("assets/textures/jade.png"), png).unwrap();
        std::fs::write(
            dir.join("server/main.luau"),
            format!("return function(host) {source} end"),
        )
        .unwrap();
        std::fs::write(dir.join("package.txt"), format!(
            "format 2\npackage demo\nversion 1.0.0\nentry main\n{requires}module server main server/main.luau\nasset texture tile assets/textures/jade.png\n"
        )).unwrap();
        let error = fixture
            .open()
            .err()
            .expect("invalid texture declaration must refuse startup");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

fn fragmented(peer: &mut TcpStream, message: &ClientMessage) {
    let mut bytes = Vec::new();
    protocol::write_client(&mut bytes, message).unwrap();
    for byte in bytes {
        peer.write_all(&[byte]).unwrap();
    }
}

fn offer(address: SocketAddr, profile: u128) -> (TcpStream, BundleIdentity) {
    let mut peer = TcpStream::connect(address).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    peer.set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    fragmented(
        &mut peer,
        &ClientMessage::Hello {
            name: "bundle".into(),
            profile,
            content_fingerprint: 0,
        },
    );
    let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut peer).unwrap() else {
        panic!("bundle offer must precede Welcome and snapshots");
    };
    (peer, identity)
}

fn closed(peer: &mut TcpStream) {
    match peer.read(&mut [0]) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::ConnectionReset | ErrorKind::BrokenPipe
            ) => {}
        result => panic!("expected disconnect without admission, got {result:?}"),
    }
}

fn welcome(peer: &mut TcpStream) {
    let (fingerprint, _) = receive_content_manifest(peer);
    fragmented(peer, &ClientMessage::ContentReady { fingerprint });
    assert!(matches!(
        protocol::read_server(peer).unwrap(),
        ServerMessage::Welcome { .. }
    ));
}

#[test]
fn bundle_gate_rejects_mismatches_and_early_play_without_blocking_healthy_join() {
    let fixture = fixture();
    gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
        for mode in 0..5 {
            let (mut peer, identity) = offer(address, 0x100 + mode);
            let wrong = BundleIdentity {
                client_runtime: crate::protocol::CLIENT_RUNTIME_VERSION,
                key: CacheKey::from_bytes([0; 32]),
                ..identity
            };
            let message = match mode {
                0 => ClientMessage::BundleRequest { identity: wrong },
                1 => ClientMessage::BundleReady { identity: wrong },
                2 => ClientMessage::BundleReady {
                    identity: BundleIdentity {
                        total_len: identity.total_len + 1,
                        ..identity
                    },
                },
                3 => ClientMessage::Ping { nonce: 1 },
                _ => ClientMessage::ContentReady { fingerprint: 0 },
            };
            fragmented(&mut peer, &message);
            closed(&mut peer);
        }
        // A peer withholding readiness never occupies simulation/player state.
        let (mut blocked, identity) = offer(address, 0x200);
        fragmented(&mut blocked, &ClientMessage::BundleRequest { identity });
        let mut received = 0;
        while received < identity.total_len as usize {
            let ServerMessage::BundlePart { offset, bytes } =
                protocol::read_server(&mut blocked).unwrap()
            else {
                panic!("play before artifact readiness");
            };
            assert_eq!(offset as usize, received);
            assert!(bytes.len() <= MAX_BUNDLE_PART);
            received += bytes.len();
        }
        let (mut healthy, identity) = offer(address, 0x201);
        crate::client::bundle::receive(&mut healthy, identity, None).unwrap();
        welcome(&mut healthy);
        // Healthy Welcome is an explicit reactor/coordinator progress barrier.
        blocked.set_nonblocking(true).unwrap();
        assert_eq!(
            blocked.read(&mut [0]).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        blocked.set_nonblocking(false).unwrap();
        fragmented(&mut blocked, &ClientMessage::BundleReady { identity });
        welcome(&mut blocked);
    });
}

#[test]
fn bundle_restart_exact_cache_and_changed_source_require_new_bytes() {
    let fixture = fixture();
    let mut cached: Option<Arc<ClientBundle>> = None;
    for round in 0..3 {
        if round == 2 {
            std::fs::write(
                fixture.0.join("packages/demo/client/view.luau"),
                "return 'changed'",
            )
            .unwrap();
        }
        let state = Box::new(fixture.open().unwrap());
        let expected = state.client_bundle.clone().unwrap();
        gameplay::serve(state, |address| {
            let (mut peer, identity) = offer(address, 0x300);
            assert_eq!(identity.key, expected.cache_key());
            let bundle =
                crate::client::bundle::receive(&mut peer, identity, cached.clone()).unwrap();
            assert_eq!(bundle.bytes(), expected.bytes());
            assert_eq!(bundle.packages()["demo"].sources.len(), 1);
            if let Some(prior) = &cached {
                assert_eq!(Arc::ptr_eq(prior, &bundle), round == 1);
            }
            // Cache-hit Ready must skip all BundlePart frames.
            welcome(&mut peer);
            cached = Some(bundle);
        });
    }
}

#[test]
fn client_verification_rejects_relay_tamper_truncation_and_reordering() {
    let fixture = fixture();
    gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
        for mode in 0..5 {
            let (mut upstream, identity) = offer(address, 0x400 + mode);
            // The real server still owns offer and part production. This relay
            // changes only the byte delivery presented to the production client.
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (mut relay, _) = listener.accept().unwrap();
            relay
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            relay
                .set_write_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let worker = thread::spawn(move || {
                assert_eq!(
                    protocol::read_client(&mut relay).unwrap(),
                    ClientMessage::BundleRequest { identity }
                );
                fragmented(&mut upstream, &ClientMessage::BundleRequest { identity });
                let mut received = 0;
                while received < identity.total_len as usize {
                    let ServerMessage::BundlePart {
                        mut offset,
                        mut bytes,
                    } = protocol::read_server(&mut upstream).unwrap()
                    else {
                        panic!("expected part");
                    };
                    received += bytes.len();
                    if mode == 1 && offset == 0 {
                        bytes[0] ^= 1;
                    }
                    if mode == 2 {
                        offset += 1;
                    }
                    if mode == 4 {
                        bytes.truncate(1);
                    }
                    let mut frame = Vec::new();
                    protocol::write_server(
                        &mut frame,
                        &ServerMessage::BundlePart { offset, bytes },
                    )
                    .unwrap();
                    if mode == 3 {
                        relay.write_all(&frame[..frame.len() / 2]).unwrap();
                        relay.shutdown(Shutdown::Write).unwrap();
                        break;
                    }
                    // Split length prefixes and payloads, without sleeps.
                    for byte in &frame[..4] {
                        relay.write_all(&[*byte]).unwrap();
                    }
                    for chunk in frame[4..].chunks(997) {
                        relay.write_all(chunk).unwrap();
                    }
                    if mode == 2 || mode == 4 {
                        break;
                    }
                }
                if mode == 0 {
                    assert_eq!(
                        protocol::read_client(&mut relay).unwrap(),
                        ClientMessage::BundleReady { identity }
                    );
                    fragmented(&mut upstream, &ClientMessage::BundleReady { identity });
                    welcome(&mut upstream);
                } else {
                    // A failed client must never send Ready, even when all
                    // advertised bytes arrived and only the digest is wrong.
                    closed(&mut relay);
                }
            });
            let result = crate::client::bundle::receive(&mut client, identity, None);
            assert_eq!(result.is_ok(), mode == 0);
            let _ = client.shutdown(Shutdown::Both);
            drop(client);
            worker.join().unwrap();
        }
    });
}

#[test]
fn bundle_wire_rejects_unbounded_lengths_before_allocation() {
    let identity = BundleIdentity {
        client_runtime: crate::protocol::CLIENT_RUNTIME_VERSION,
        key: CacheKey::from_bytes([0; 32]),
        total_len: 1,
    };
    let mut bytes = Vec::new();
    protocol::write_server(&mut bytes, &ServerMessage::BundleOffer { identity }).unwrap();
    let end = bytes.len();
    bytes[end - 4..].copy_from_slice(&((MAX_BUNDLE_BYTES + 1) as u32).to_le_bytes());
    assert!(protocol::read_server(bytes.as_slice()).is_err());
    for message in [
        ServerMessage::BundlePart {
            offset: 0,
            bytes: vec![0; MAX_BUNDLE_PART + 1],
        },
        ServerMessage::BundlePart {
            offset: u32::MAX,
            bytes: vec![0],
        },
        ServerMessage::BundlePart {
            offset: 0,
            bytes: Vec::new(),
        },
    ] {
        assert!(protocol::write_server(&mut Vec::new(), &message).is_err());
    }
}

#[test]
fn production_network_installs_session_bundle_before_exposing_welcome() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    // An empty local package root exports a canonical empty artifact while
    // retaining the built-in catalog.
    let fixture = Fixture::new();
    let mut prior = None;
    for _ in 0..2 {
        let state = Box::new(fixture.open().unwrap());
        let expected = state.client_bundle.clone().unwrap();
        gameplay::serve(state, |address| {
            let installed = crate::client::connect_bundle_probe(&address.to_string(), 0x500)
                .unwrap()
                .unwrap();
            assert_eq!(installed.bytes(), expected.bytes());
            if let Some(prior) = &prior {
                assert!(
                    Arc::ptr_eq(prior, &installed),
                    "production cache was not reused"
                );
            }
            prior = Some(installed);
        });
    }
    // A later unmodded connection must not retain the previous session artifact.
    let plain = Fixture::new();
    let state = crate::server::server_state_with_startup(
        7,
        plain.0.join("save"),
        2,
        ServerStartup::new(Arc::new(Catalog::builtins())),
    )
    .unwrap();
    gameplay::serve(Box::new(state), |address| {
        assert!(
            crate::client::connect_bundle_probe(&address.to_string(), 0x501)
                .unwrap()
                .is_none()
        );
    });
    // Package identity is now declared as inert metadata. The hostile client
    // source remains unexecuted, and opaque image bytes remain undecoded.
    let modded = self::fixture();
    gameplay::serve(Box::new(modded.open().unwrap()), |address| {
        let catalog = crate::client::connect_catalog_probe(&address.to_string(), 0x600).unwrap();
        assert!(
            crate::content::ContentManifest::from_catalog(&catalog)
                .entries
                .iter()
                .any(|e| e.key == "demo:package")
        );
    });
}

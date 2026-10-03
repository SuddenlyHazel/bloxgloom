//! Real nonblocking listeners remain live throughout client retirement/retry.
use super::*;

fn copy_fixture(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn modded_join_failure_retry_and_switch_retire_session_resources() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let combined = Fixture::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    copy_fixture(
        &root.join("combined-mod/packages/verdant"),
        &combined.0.join("packages/verdant"),
    );
    copy_fixture(
        &root.join("effect-packages/sepia"),
        &combined.0.join("packages/sepia"),
    );
    let broken = bundle_ui::startup_fixture(
        "return function(host) host.set_text('uidemo:welcome/title', 'must not leak'); error('deliberate join failure') end",
    );
    let ui = bundle_ui::startup_fixture(
        "return function(host) host.set_text('uidemo:welcome/title', 'different server') end",
    );
    gameplay::serve(Box::new(combined.open().unwrap()), |combined_addr| {
        gameplay::serve(Box::new(broken.open().unwrap()), |broken_addr| {
            gameplay::serve(Box::new(ui.open().unwrap()), |ui_addr| {
                crate::client::exercise_join_lifecycle(
                    &combined_addr.to_string(),
                    &broken_addr.to_string(),
                    &ui_addr.to_string(),
                    combined.0.join("client-config"),
                );
            });
        });
    });
}

#[test]
fn readiness_failure_names_stage_closes_socket_and_allows_retry() {
    use std::io::Read;
    let fixture = Fixture::new();
    let state = crate::server::server_state_with_startup(
        7,
        fixture.0.join("save"),
        2,
        ServerStartup::new(Arc::new(Catalog::builtins())),
    )
    .unwrap();
    gameplay::serve(Box::new(state), |address| {
        // The upstream is the production nonblocking listener. The relay only
        // corrupts the final readiness response, after the real server admits.
        let relay = TcpListener::bind("127.0.0.1:0").unwrap();
        let relay_address = relay.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let (mut downstream, _) = relay.accept().unwrap();
            let mut upstream = TcpStream::connect(address).unwrap();
            for stream in [&downstream, &upstream] {
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
            }
            let hello = protocol::read_client(&mut downstream).unwrap();
            protocol::write_client(&mut upstream, &hello).unwrap();
            loop {
                let message = protocol::read_server(&mut upstream).unwrap();
                let ServerMessage::ContentManifestPart {
                    total_len,
                    offset,
                    ref bytes,
                    ..
                } = message
                else {
                    panic!("expected real manifest");
                };
                let last = offset as usize + bytes.len() == total_len as usize;
                protocol::write_server(&mut downstream, &message).unwrap();
                if last {
                    break;
                }
            }
            let ready = protocol::read_client(&mut downstream).unwrap();
            assert!(matches!(ready, ClientMessage::ContentReady { .. }));
            protocol::write_client(&mut upstream, &ready).unwrap();
            assert!(matches!(
                protocol::read_server(&mut upstream).unwrap(),
                ServerMessage::Welcome { .. }
            ));
            protocol::write_server(&mut downstream, &ServerMessage::Pong { nonce: 1 }).unwrap();
            let mut byte = [0u8; 1];
            assert_eq!(
                downstream.read(&mut byte).unwrap(),
                0,
                "failed join socket retained"
            );
        });
        let error =
            crate::client::connect_catalog_probe(&relay_address.to_string(), 0x11fec8).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            error
                .to_string()
                .contains("server readiness acknowledgement"),
            "{error}"
        );
        assert!(error.to_string().contains("expected Welcome"), "{error}");
        worker.join().unwrap();
        crate::client::connect_catalog_probe(&address.to_string(), 0x11fec8).unwrap();
    });
}

#[test]
fn dormant_client_and_shared_syntax_errors_fail_before_play_and_allow_retry() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    for (module, path) in [
        ("view", "client/view.luau"),
        ("helper", "shared/helper.luau"),
    ] {
        let fixture = bundle_ui::startup_fixture("return function(_) end");
        std::fs::write(
            fixture.0.join("packages/uidemo").join(path),
            "return function( broken syntax",
        )
        .unwrap();
        gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
            let error =
                crate::client::connect_bundle_probe(&address.to_string(), 0xface).unwrap_err();
            let text = error.to_string();
            assert!(text.contains("package client startup"), "{text}");
            assert!(text.contains(&format!("uidemo@1.0.0:{module}")), "{text}");
        });
        std::fs::write(
            fixture.0.join("packages/uidemo").join(path),
            "return function() error('dormant module must not execute') end",
        )
        .unwrap();
        // Repair client behavior and retry the existing save without changing
        // its declared persistent contracts.
        gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
            crate::client::connect_bundle_probe(&address.to_string(), 0xface).unwrap();
        });
    }
}

#[test]
fn incompatible_runtime_offer_fails_before_request_even_with_verified_cache() {
    use std::io::Read;
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = bundle_ui::startup_fixture("return function(_) end");
    gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
        // Populate the real process cache first. A runtime contract mismatch
        // must still fail before requesting bytes or sending BundleReady.
        crate::client::connect_bundle_probe(&address.to_string(), 0xfeed).unwrap();
        let relay = TcpListener::bind("127.0.0.1:0").unwrap();
        let relay_address = relay.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let (mut downstream, _) = relay.accept().unwrap();
            let mut upstream = TcpStream::connect(address).unwrap();
            for stream in [&downstream, &upstream] {
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
            }
            let hello = protocol::read_client(&mut downstream).unwrap();
            protocol::write_client(&mut upstream, &hello).unwrap();
            let ServerMessage::BundleOffer { mut identity } =
                protocol::read_server(&mut upstream).unwrap()
            else {
                panic!("expected offer")
            };
            identity.client_runtime += 1;
            protocol::write_server(&mut downstream, &ServerMessage::BundleOffer { identity })
                .unwrap();
            assert_eq!(
                downstream.read(&mut [0]).unwrap(),
                0,
                "incompatible client requested or acknowledged bundle"
            );
        });
        let error =
            crate::client::connect_bundle_probe(&relay_address.to_string(), 0xbeef).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            error
                .to_string()
                .contains("package download and verification"),
            "{error}"
        );
        assert!(
            error.to_string().contains(&format!(
                "runtime contract {}; this client supports {}",
                crate::protocol::CLIENT_RUNTIME_VERSION + 1,
                crate::protocol::CLIENT_RUNTIME_VERSION
            )),
            "{error}"
        );
        worker.join().unwrap();
        crate::client::connect_bundle_probe(&address.to_string(), 0xbeef).unwrap();
    });
}

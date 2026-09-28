//! Real nonblocking listener -> client install/cache -> local UI session.
use super::*;
use crate::inventory::Stack;
use bloxgloom_host_api::actions::Request;
use std::time::Instant;

#[path = "bundle_ui/target_actions.rs"]
mod target_actions;

// Format-2 fixture copied into an isolated save root; server entry and event
// handler remain unchanged, while a downloaded startup module imports a shared
// helper and registers session-only presentation text.
fn startup_fixture(source: &str) -> Fixture {
    let fixture = Fixture::new();
    let original =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/packages/uidemo");
    let target = fixture.0.join("packages/uidemo");
    for file in [
        "server/main.luau",
        "server/store.luau",
        "client/view.luau",
        "assets/ui/icon.png",
        "assets/ui/field.json",
        "assets/ui/icon-style.json",
        "assets/ui/welcome.json",
        "assets/ui/panel.json",
        "assets/ui/button.json",
        "assets/ui/label.json",
        "assets/fonts/body.ttf",
    ] {
        let path = target.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(original.join(file), path).unwrap();
    }
    std::fs::create_dir_all(target.join("shared")).unwrap();
    std::fs::write(
        target.join("shared/helper.luau"),
        "return function() return 'from downloaded shared module' end",
    )
    .unwrap();
    std::fs::write(target.join("client/client_startup.luau"), source).unwrap();
    let mut manifest = std::fs::read_to_string(original.join("package.txt")).unwrap();
    manifest.push_str("module client client_startup client/client_startup.luau\nmodule shared helper shared/helper.luau\n");
    std::fs::write(target.join("package.txt"), manifest).unwrap();
    fixture
}

#[test]
fn downloaded_client_startup_is_session_scoped_across_reconnect_and_switch() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    for (source, expected) in [
        (
            "return function(host) host.set_text('uidemo:welcome/title', import('uidemo:helper')()); host.set_state('uidemo:welcome', 'first') end",
            "from downloaded shared module",
        ),
        (
            "return function(host) host.set_text('uidemo:welcome/title', 'second server') end",
            "second server",
        ),
    ] {
        let fixture = startup_fixture(source);
        gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
            for profile in [0xab01, 0xab02] {
                crate::client::connect_ui_probe(
                    &address.to_string(),
                    profile,
                    |bundle, session| {
                        assert!(
                            bundle.packages()["uidemo"]
                                .sources
                                .contains_key("client_startup")
                        );
                        assert!(!bundle.packages()["uidemo"].sources.contains_key("main"));
                        assert_eq!(session.text_at(1), expected);
                        session.next_document();
                        assert_eq!(session.text_at(1), expected);
                    },
                )
                .unwrap();
            }
        });
    }
}

#[test]
fn client_startup_failure_refuses_content_ready_with_package_and_module() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    for source in [
        "return function(host) host.set_text('uidemo:welcome/title', import('uidemo:main')) end",
        "return function(host) host.set_text('uidemo:welcome/missing', 'bad') end",
        "return function(_) while true do end end",
    ] {
        let fixture = startup_fixture(source);
        gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
            let error =
                crate::client::connect_bundle_probe(&address.to_string(), 0xab03).unwrap_err();
            let message = error.to_string();
            assert!(
                message.contains("uidemo") && message.contains("client_startup"),
                "{message}"
            );
        });
    }
}

#[test]
fn startup_worker_imports_exact_direct_dependencies_with_lexical_visibility() {
    let fixture = startup_fixture(
        "return function(host) host.set_text('uidemo:welcome/title', import('helper:public')()) end",
    );
    let helper = fixture.0.join("packages/helper");
    std::fs::create_dir_all(helper.join("server")).unwrap();
    std::fs::create_dir_all(helper.join("shared")).unwrap();
    std::fs::write(helper.join("server/main.luau"), "return function(_) end").unwrap();
    std::fs::write(
        helper.join("shared/public.luau"),
        "return function() return 'direct dependency' end",
    )
    .unwrap();
    std::fs::write(helper.join("package.txt"), "format 2\npackage helper\nversion 1.0.0\nentry main\nmodule server main server/main.luau\nmodule shared public shared/public.luau\n").unwrap();
    let manifest = fixture.0.join("packages/uidemo/package.txt");
    let original = std::fs::read_to_string(&manifest).unwrap();
    // Without the exact direct dependency, even a present verified module is inaccessible.
    let without =
        crate::server::script::package::PackageSnapshot::discover(&fixture.0.join("packages"))
            .unwrap();
    let bundle = Arc::clone(without.client_bundle());
    let err = crate::client::startup::prepare(bundle)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("uidemo@1.0.0:client_startup") && err.contains("inaccessible"),
        "{err}"
    );
    drop(without);
    std::fs::write(&manifest, format!("{original}dependency helper 1.0.0\n")).unwrap();
    let snapshot =
        crate::server::script::package::PackageSnapshot::discover(&fixture.0.join("packages"))
            .unwrap();
    let bundle = Arc::clone(snapshot.client_bundle());
    let result = crate::client::startup::prepare(Arc::clone(&bundle)).unwrap();
    assert_eq!(result.texts["uidemo:welcome/title"], "direct dependency");
    // An exported function uses its defining helper package's imports, not
    // uidemo's authority; helper cannot import uidemo without its own edge.
    std::fs::write(
        helper.join("shared/public.luau"),
        "return function() return import('uidemo:helper')() end",
    )
    .unwrap();
    let snapshot =
        crate::server::script::package::PackageSnapshot::discover(&fixture.0.join("packages"))
            .unwrap();
    let error = crate::client::startup::prepare(Arc::clone(snapshot.client_bundle()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("helper@1.0.0:public") && error.contains("inaccessible"),
        "{error}"
    );
    // The previous immutable artifact remains usable after local files change.
    assert_eq!(
        crate::client::startup::prepare(bundle).unwrap().texts["uidemo:welcome/title"],
        "direct dependency"
    );
}

#[test]
fn startup_worker_discards_partial_registration_and_caught_limit() {
    let fixture = startup_fixture(
        "return function(host) host.set_text('uidemo:welcome/title', 'partial'); pcall(function() while true do end end) end",
    );
    let snapshot =
        crate::server::script::package::PackageSnapshot::discover(&fixture.0.join("packages"))
            .unwrap();
    let error = crate::client::startup::prepare(Arc::clone(snapshot.client_bundle()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("uidemo@1.0.0:client_startup") && error.contains("limit"),
        "{error}"
    );
    std::fs::write(
        fixture.0.join("packages/uidemo/client/client_startup.luau"),
        "return function(host) host.set_text('uidemo:welcome/title', 'new session') end",
    )
    .unwrap();
    let next =
        crate::server::script::package::PackageSnapshot::discover(&fixture.0.join("packages"))
            .unwrap();
    let state = crate::client::startup::prepare(Arc::clone(next.client_bundle())).unwrap();
    assert_eq!(state.texts["uidemo:welcome/title"], "new session");
}

#[test]
fn package_ui_worker_dispatches_on_join_and_resets_on_cached_reconnect() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/packages"),
        )
        .unwrap();
    let state =
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap();
    let key = state.client_bundle.as_ref().unwrap().cache_key();
    let fingerprint = state.world.catalog_arc().fingerprint();
    gameplay::serve(Box::new(state), |address| {
        for profile in [0x911, 0x912] {
            crate::client::connect_ui_probe(&address.to_string(), profile, |bundle, session| {
                assert_eq!(bundle.cache_key(), key);
                session.resize(640, 360, 1.0);
                assert!(session.focused_id().is_none());
                assert_eq!(session.text_at(1), "Welcome to the garden");
                assert_eq!(session.text_at(4), "Moss & stone");
                session.tab(false);
                assert_eq!(session.focused_id(), Some("uidemo:welcome/name"));
                assert_eq!(session.event(), Some("uidemo:name-changed"));
                session.edit(false, Some(" local edit"));
                session.wait_for_presentation().unwrap();
                assert_eq!(session.text_at(1), "Garden: Moss & stone local edit");
                session.tab(false);
                session.activate();
                session.wait_for_presentation().unwrap();
                assert_eq!(session.take_action().as_deref(), Some("uidemo:store"));
                // The immutable artifact and authoritative catalog remain unchanged.
                assert_eq!(bundle.cache_key(), key);
                assert_eq!(bundle.session_catalog().unwrap().fingerprint(), fingerprint);
            })
            .unwrap();
        }
    });
}

#[test]
fn authored_button_reaches_authoritative_receipt_and_durable_inventory() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    const PROFILE: u128 = 0x9191;
    let fixture = Fixture::new();
    let packages = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/packages");
    let mut old_request = None;
    for restarted in [false, true] {
        let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
            .with_local_packages(&packages)
            .unwrap();
        let state = crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup)
            .unwrap();
        let catalog = state.world.catalog_arc();
        let bundle = Arc::clone(state.client_bundle.as_ref().unwrap());
        let position_path = fixture
            .0
            .join("save/players")
            .join(format!("{PROFILE:032x}.pos"));
        let old_position_write = std::fs::metadata(&position_path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        let stick = catalog.item_by_key("bloxgloom:stick").unwrap();
        if !restarted {
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::new(stick, 3));
            state.inventory_store.save(PROFILE, &inventory).unwrap();
        }
        gameplay::serve(Box::new(state), |address| {
            let mut session = crate::ui::authored::Session::new(Arc::clone(bundle.ui().unwrap()));
            session.resize(640, 360, 1.0);
            session.tab(false);
            session.tab(false);
            assert_eq!(session.event(), Some("uidemo:store"));

            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut stream,
                &ClientMessage::Hello {
                    name: "ui-action".into(),
                    profile: PROFILE,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut stream);
            protocol::write_client(&mut stream, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let mut inventory = Inventory::default();
            let mut epoch = 0;
            let deadline = Instant::now() + Duration::from_secs(10);
            let initial = if restarted { 1 } else { 3 };
            while epoch == 0 || inventory.slots[0].as_ref().map(|s| s.count) != Some(initial) {
                assert!(Instant::now() < deadline);
                match protocol::read_server_with_catalog(&mut stream, &catalog).unwrap() {
                    ServerMessage::ActionSession { epoch: id, .. } => epoch = id,
                    ServerMessage::Inventory { revision, slots } => {
                        inventory = Inventory { revision, slots }
                    }
                    _ => {}
                }
            }
            let mut sequence = 1u64;
            let send = |mut stream: &mut TcpStream,
                        request: &ClientMessage,
                        session: &mut crate::ui::authored::Session,
                        inventory: &mut Inventory| {
                protocol::write_client_with_catalog(&mut stream, request, &catalog).unwrap();
                let expected = match request {
                    ClientMessage::EntityInteract { action_id, .. } => *action_id,
                    _ => unreachable!(),
                };
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    assert!(Instant::now() < deadline, "UI action receipt missing");
                    match protocol::read_server_with_catalog(&mut stream, &catalog).unwrap() {
                        ServerMessage::ActionResult {
                            action_id,
                            accepted,
                            reason,
                        } if action_id == expected => {
                            session.action_result(action_id, accepted, &reason);
                            return (accepted, reason);
                        }
                        ServerMessage::Inventory { revision, slots }
                            if revision >= inventory.revision =>
                        {
                            *inventory = Inventory { revision, slots }
                        }
                        _ => {}
                    }
                }
            };
            if restarted {
                let (accepted, _) = send(
                    &mut stream,
                    old_request.as_ref().unwrap(),
                    &mut session,
                    &mut inventory,
                );
                assert!(!accepted, "old session request must not replay");
            }
            // No script effect on click: only an owned semantic key is emitted.
            session.activate();
            session.wait_for_presentation().unwrap();
            assert_eq!(session.text_at(1), "Welcome to the garden");
            let key = session.take_action().unwrap();
            let action_id = (u128::from(epoch) << 64) | u128::from(sequence);
            sequence += 1;
            let request = crate::client::compose_package_action(
                &catalog,
                &key,
                0,
                &inventory,
                [0, 0, 0],
                None,
                action_id,
            )
            .unwrap();
            let ClientMessage::EntityInteract { payload, .. } = &request else {
                unreachable!()
            };
            let decoded = Request::decode(payload).unwrap();
            assert_eq!(decoded.arguments, Vec::<u8>::new());
            assert_eq!(decoded.inventory_revision, inventory.revision);
            session.action_submitted(action_id);
            assert_eq!(session.feedback(), Some("WAITING FOR SERVER"));
            assert!(send(&mut stream, &request, &mut session, &mut inventory).0);
            assert_eq!(session.feedback(), Some("SERVER APPLIED ACTION"));
            let expected_count = if restarted { 0 } else { 2 };
            while inventory.slots[0].as_ref().map_or(0, |s| s.count) != expected_count {
                assert!(Instant::now() < deadline);
                if let ServerMessage::Inventory { revision, slots } =
                    protocol::read_server_with_catalog(&mut stream, &catalog).unwrap()
                {
                    inventory = Inventory { revision, slots };
                }
            }
            assert_eq!(
                inventory.slots[1].as_ref().unwrap().count,
                if restarted { 3 } else { 1 }
            );
            if !restarted {
                assert!(
                    send(&mut stream, &request, &mut session, &mut inventory).0,
                    "duplicate receipt"
                );
                assert_eq!(inventory.slots[0].as_ref().unwrap().count, 2);
                let mut stale = request.clone();
                if let ClientMessage::EntityInteract { action_id, .. } = &mut stale {
                    *action_id = (u128::from(epoch) << 64) | u128::from(sequence);
                }
                sequence += 1;
                session.action_submitted((u128::from(epoch) << 64) | u128::from(sequence - 1));
                assert!(!send(&mut stream, &stale, &mut session, &mut inventory).0);
                assert!(session.feedback().unwrap().starts_with("SERVER DENIED:"));
                // A fresh revision but slot 2 is still denied by server script.
                let denied_id = (u128::from(epoch) << 64) | u128::from(sequence);
                let denied = crate::client::compose_package_action(
                    &catalog,
                    &key,
                    1,
                    &inventory,
                    [0, 0, 0],
                    None,
                    denied_id,
                )
                .unwrap();
                session.action_submitted(denied_id);
                assert!(!send(&mut stream, &denied, &mut session, &mut inventory).0);
                assert_eq!(inventory.slots[0].as_ref().unwrap().count, 2);
                // Retry with a fresh ID and current revision: denied requests
                // cannot silently apply, and the subsequent valid one can.
                let retry_id = (u128::from(epoch) << 64) | u128::from(sequence + 1);
                let retry = crate::client::compose_package_action(
                    &catalog,
                    &key,
                    0,
                    &inventory,
                    [0, 0, 0],
                    None,
                    retry_id,
                )
                .unwrap();
                session.action_submitted(retry_id);
                assert!(send(&mut stream, &retry, &mut session, &mut inventory).0);
                while inventory.slots[0].as_ref().map(|s| s.count) != Some(1) {
                    assert!(Instant::now() < deadline);
                    if let ServerMessage::Inventory { revision, slots } =
                        protocol::read_server_with_catalog(&mut stream, &catalog).unwrap()
                    {
                        inventory = Inventory { revision, slots };
                    }
                }
                assert_eq!(inventory.slots[1].as_ref().unwrap().count, 2);
                old_request = Some(request);
            }
            let _ = stream.shutdown(Shutdown::Both);
            // Let the real nonblocking listener finish Leave (including its
            // position save) before asking the harness to stop the coordinator.
            let leave_deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let saved = std::fs::metadata(&position_path)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok());
                if saved.is_some() && saved != old_position_write {
                    break;
                }
                assert!(
                    Instant::now() < leave_deadline,
                    "player leave was not persisted"
                );
                std::thread::yield_now();
            }
        });
    }
}

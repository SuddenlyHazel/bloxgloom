//! Real nonblocking listener -> client install/cache -> local UI session.
use super::*;
use crate::inventory::Stack;
use bloxgloom_host_api::actions::Request;
use std::time::Instant;

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

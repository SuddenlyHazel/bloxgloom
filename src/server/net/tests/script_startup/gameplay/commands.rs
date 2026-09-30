//! Command descriptors are negotiated data, not client-side script authority.
use super::*;
use bloxgloom_host_api::actions::{Command, CommandArgument, CommandPermission, Target};
#[path = "commands/builtins.rs"]
mod builtins;
#[path = "commands/clock.rs"]
mod clock;
#[path = "commands/registration.rs"]
mod registration;

fn declaration(permission: &str) -> String {
    format!(
        "return function(h) h.register_action('demo:shift',1,'Shift','empty',nil,'demo:action',{{permission='{permission}'}}) end"
    )
}

fn command_request(peer: &mut Peer) -> ClientMessage {
    let mut message = peer.request(0);
    if let ClientMessage::EntityInteract { payload, .. } = &mut message {
        let mut request = Request::decode(payload).unwrap();
        request.arguments.clear();
        *payload = request.encode().unwrap();
    }
    message
}

#[test]
fn negotiated_commands_enforce_permission_and_zero_args_with_receipts_and_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    for (name, permission) in [
        ("Player", CommandPermission::Player),
        ("Admin", CommandPermission::Admin),
    ] {
        let fixture = Fixture::new();
        // A plain transfer deliberately does not require admin in the handler:
        // denial must come from the frozen descriptor before handler invocation.
        fixture.action(
            &declaration(name),
            r#"
            -- PRIVATE-COMMAND-SOURCE
            return function(c,e)
                assert(e.arguments == '' and e.cell == nil)
                assert(c.transfer(0,1,1))
            end
        "#,
        );
        let mut moved = 0;
        let mut previous = None;
        for round in 0..3 {
            let mut state = Box::new(fixture.open().unwrap());
            state.admin_profile = (round == 1).then_some(PROFILE);
            let expected = state.world.catalog_arc();
            if round == 0 {
                let mut inventory = Inventory::default();
                inventory.slots[0] = Some(Stack::new(crate::items::STICK, 4));
                state.inventory_store.save(PROFILE, &inventory).unwrap();
            }
            let bundle = state.client_bundle.as_ref().unwrap();
            assert!(bundle.packages().values().all(|p| p.sources.is_empty()));
            let private = b"PRIVATE-COMMAND-SOURCE";
            assert!(!bundle.bytes().windows(private.len()).any(|b| b == private));
            serve(state, |address| {
                // Production client bundle negotiation/remapping, not a cloned
                // server catalog or execution of the private startup source.
                let negotiated =
                    crate::client::connect_catalog_probe(&address.to_string(), PROFILE + 1)
                        .unwrap();
                assert_eq!(negotiated.fingerprint(), expected.fingerprint());
                let action = negotiated
                    .discover_actions(&Target::Empty)
                    .find(|action| action.key == "demo:shift")
                    .unwrap();
                assert_eq!(
                    action.command,
                    Some(Command {
                        permission: permission.clone(),
                        arguments: vec![],
                    })
                );
                assert!(
                    negotiated
                        .gameplay_handler(
                            bloxgloom_host_api::gameplay::EventKind::ActionRequested,
                            "demo:shift"
                        )
                        .is_none()
                );
                let mut peer = Peer::connect(address, negotiated);
                peer.inventory_at(4 - moved);
                if let Some(old) = &previous {
                    assert!(!peer.send(old).0, "old session cannot replay a command");
                }
                let authorized = permission == CommandPermission::Player || round == 1;
                if authorized {
                    // Both a short forged argument and the codec's full allowed
                    // payload must fail at command planning, not in the script.
                    for size in [1, 130] {
                        let mut bad = command_request(&mut peer);
                        if let ClientMessage::EntityInteract { payload, .. } = &mut bad {
                            let mut request = Request::decode(payload).unwrap();
                            request.arguments.resize(size, 0);
                            *payload = request.encode().unwrap();
                        }
                        let (accepted, reason) = peer.send(&bad);
                        assert!(!accepted, "{reason}");
                        assert!(reason.contains("invalid command arguments"), "{reason}");
                        assert!(!peer.send(&bad).0, "denial receipt cannot execute later");
                    }
                }
                let request = command_request(&mut peer);
                let (accepted, reason) = peer.send(&request);
                assert_eq!(accepted, authorized, "{reason}");
                if authorized {
                    moved += 1;
                    peer.inventory_at(4 - moved);
                    assert!(peer.send(&request).0, "same-session receipt replay");
                    assert_eq!(peer.inventory.slots[1].as_ref().unwrap().count, moved);
                } else {
                    assert!(reason.contains("requires admin"), "{reason}");
                    assert!(!peer.send(&request).0);
                }
                previous = Some(request);
            });
            // Recovery proves that neither denied commands nor receipt replay
            // published a transfer hidden by a later successful transaction.
            let recovered = fixture.open().unwrap();
            let inventory = recovered.inventory_store.load(PROFILE).unwrap();
            assert_eq!(inventory.slots[0].as_ref().unwrap().count, 4 - moved);
            assert_eq!(
                inventory.slots[1].as_ref().map(|s| s.count),
                (moved != 0).then_some(moved)
            );
        }
    }
}

#[test]
fn invalid_command_declarations_poison_startup_even_when_caught() {
    let fixture = Fixture::new();
    let valid = declaration("Player");
    for bad in [
        valid.replace("{permission='Player'}", "{}"),
        valid.replace("'Player'", "'Owner'"),
        valid.replace("{permission='Player'}", "true"),
        valid.replace(
            "{permission='Player'}",
            "{permission='Player', arguments=true}",
        ),
        valid.replace(
            "{permission='Player'}",
            "{permission='Player', alias='shift'}",
        ),
        valid.replace("{permission='Player'}", "{permission='Player'}, 0"),
        valid.replace("'empty',nil", "'item','bloxgloom:stick'"),
        valid.replace("'empty',nil", "'block','bloxgloom:stone'"),
        valid.replace("demo:shift", &format!("demo:{}", "a".repeat(124))),
        valid.replace("demo:shift", "shift"),
        valid.replace("demo:shift", "other:shift"),
        valid.replace("{permission='Player'}", "{permission='Player',arguments={{kind='count',default=0}}}"),
        valid.replace("{permission='Player'}", "{permission='Player',arguments={{kind='count',default=129}}}"),
        valid.replace("{permission='Player'}", "{permission='Player',arguments={{kind='item_key',max_bytes=129}}}"),
        valid.replace("{permission='Player'}", "{permission='Player',arguments={{kind='item_key'}}}"),
        valid.replace("{permission='Player'}", "{permission='Player',arguments={{kind='item_key',max_bytes=128},{kind='entity_key',max_bytes=128}}}"),
        valid.replace("{permission='Player'}", "{permission='Player',arguments={{kind='count',default=1},{kind='item_key',max_bytes=32}}}"),
        valid.replace("{permission='Player'}", "{permission='Player',arguments={[2]={kind='count'}}}"),
        valid.replace("{permission='Player'}", "{permission='Player',arguments={{kind='count',alias='n'}}}"),
        valid.replace("{permission='Player'}", "{permission='Player',arguments={{kind='number'}}}"),
    ] {
        let caught = bad
            .replace(
                "return function(h) ",
                "return function(h) pcall(function() ",
            )
            .replace(") end", ") end) end");
        fixture.action(&caught, "return function() end");
        assert!(
            fixture.open().is_err(),
            "bad declaration accepted: {caught}"
        );
        assert!(!fixture.0.join("save").exists());
    }
    fixture.action(&valid, "return function() end");
    drop(fixture.open().unwrap());
    let manifest = std::fs::read(fixture.0.join("save/content.map")).unwrap();
    fixture.action(&declaration("Admin"), "return function() end");
    assert!(
        fixture.open().is_err(),
        "permission change must invalidate saved identity"
    );
    assert_eq!(
        std::fs::read(fixture.0.join("save/content.map")).unwrap(),
        manifest
    );
    fixture.action(
        &valid.replace(
            "{permission='Player'}",
            "{permission='Player',arguments={{kind='count',default=1}}}",
        ),
        "return function() end",
    );
    assert!(
        fixture.open().is_err(),
        "schema change must invalidate saved identity"
    );
}

fn typed_request(peer: &mut Peer, key: &str, arguments: Vec<u8>) -> ClientMessage {
    let mut message = peer.request(0);
    if let ClientMessage::EntityInteract { payload, .. } = &mut message {
        let mut request = Request::decode(payload).unwrap();
        request.key = key.into();
        request.arguments = arguments;
        *payload = request.encode().unwrap();
    }
    message
}

#[test]
fn typed_mod_command_negotiates_order_validates_before_handler_and_recovers_once() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    fixture.action(
        "return function(h) h.register_action('demo:shift',1,'Shift','empty',nil,'demo:action',{permission='Admin',arguments={{kind='entity_key',max_bytes=48},{kind='item_key',max_bytes=48},{kind='count',default=1}}}) end",
        // Handler does not validate arguments or require admin: frozen metadata
        // alone must deny before invocation (and thus before any transfer).
        "return function(c,e) assert(c.transfer(0,1,1)) end",
    );
    let mut old = None;
    for round in 0_usize..3 {
        let mut state = Box::new(fixture.open().unwrap());
        state.admin_profile = (round != 0).then_some(PROFILE);
        let catalog = state.world.catalog_arc();
        if round == 0 {
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::new(crate::items::STICK, 4));
            state.inventory_store.save(PROFILE, &inventory).unwrap();
        }
        let moved = round.saturating_sub(1) as u16;
        serve(state, |address| {
            let negotiated =
                crate::client::connect_catalog_probe(&address.to_string(), PROFILE + 1).unwrap();
            assert_eq!(negotiated.fingerprint(), catalog.fingerprint());
            let schema = negotiated
                .action("demo:shift")
                .unwrap()
                .command
                .clone()
                .unwrap();
            assert_eq!(
                schema.arguments,
                vec![
                    CommandArgument::EntityKey { max_bytes: 48 },
                    CommandArgument::ItemKey { max_bytes: 48 },
                    CommandArgument::Count { default: Some(1) },
                ]
            );
            let valid = schema
                .encode_arguments(&["bloxgloom:mossbun", "bloxgloom:stick"])
                .unwrap();
            let mut peer = Peer::connect(address, negotiated);
            peer.inventory_at(4 - moved);
            if let Some(old) = &old {
                assert!(!peer.send(old).0);
            }
            if round != 0 {
                let mut zero = valid.clone();
                *zero.last_mut().unwrap() = 0;
                let mut too_many = valid.clone();
                too_many.push(1);
                let mut oversized = valid.clone();
                oversized[0] = 49;
                let mut uppercase = valid.clone();
                uppercase[1] = b'B';
                for arguments in [
                    vec![],
                    valid[..valid.len() - 1].to_vec(),
                    zero,
                    too_many,
                    oversized,
                    uppercase,
                    schema
                        .encode_arguments(&["demo:missing", "bloxgloom:stick"])
                        .unwrap(),
                    schema
                        .encode_arguments(&["bloxgloom:mossbun", "demo:missing"])
                        .unwrap(),
                    schema
                        .encode_arguments(&["bloxgloom:stick", "bloxgloom:mossbun"])
                        .unwrap(),
                ] {
                    let forged = typed_request(&mut peer, "demo:shift", arguments);
                    let (accepted, reason) = peer.send(&forged);
                    assert!(!accepted, "{reason}");
                    assert!(reason.contains("invalid command arguments"), "{reason}");
                    assert!(!peer.send(&forged).0);
                }
            }
            let request = typed_request(&mut peer, "demo:shift", valid);
            let (accepted, reason) = peer.send(&request);
            assert_eq!(accepted, round != 0, "{reason}");
            if accepted {
                peer.inventory_at(3 - moved);
                assert!(peer.send(&request).0);
                assert_eq!(peer.inventory.slots[1].as_ref().unwrap().count, moved + 1);
            } else {
                assert!(reason.contains("requires admin"), "{reason}");
            }
            old = Some(request);
        });
        let recovered = fixture.open().unwrap();
        let inventory = recovered.inventory_store.load(PROFILE).unwrap();
        assert_eq!(inventory.slots[0].as_ref().unwrap().count, 4 - round as u16);
        assert_eq!(
            inventory.slots[1].as_ref().map(|s| s.count),
            (round != 0).then_some(round as u16)
        );
    }
}

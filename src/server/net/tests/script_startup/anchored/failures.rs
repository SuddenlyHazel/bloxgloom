//! Invalid Luau behavior must never partially debit or publish anchored state.
use super::*;

#[path = "callbacks.rs"]
mod callbacks;
#[path = "registration.rs"]
mod registration;

fn package(fixture: &Fixture, main: &str, behavior: &str, capability: bool) {
    let source = packages().join("counter");
    let target = fixture.0.join("packages/counter");
    std::fs::create_dir_all(target.join("server")).unwrap();
    std::fs::create_dir_all(target.join("assets/textures")).unwrap();
    let manifest = std::fs::read_to_string(source.join("package.txt")).unwrap();
    std::fs::write(
        target.join("package.txt"),
        if capability {
            manifest
        } else {
            manifest.replace("requires bloxgloom:anchored_entities/v1\n", "")
        },
    )
    .unwrap();
    std::fs::copy(
        source.join("assets/textures/counter.png"),
        target.join("assets/textures/counter.png"),
    )
    .unwrap();
    std::fs::write(target.join("server/main.luau"), main).unwrap();
    std::fs::write(target.join("server/behavior.luau"), behavior).unwrap();
}

fn sources() -> (String, String) {
    let source = packages().join("counter/server");
    (
        std::fs::read_to_string(source.join("main.luau")).unwrap(),
        std::fs::read_to_string(source.join("behavior.luau")).unwrap(),
    )
}

#[test]
fn luau_anchored_registration_requires_capability_and_rejects_caught_invalid_geometry_before_save()
{
    let (main, behavior) = sources();
    let fixture = Fixture::new();
    package(&fixture, &main, &behavior, false);
    assert!(fixture.open().is_err());
    assert!(!fixture.0.join("save").exists());

    let fixture = Fixture::new();
    let bad = main
        .replace("{offset = {0, 1, 0}", "{offset = {0, 0, 0}")
        .replace(
            "h.register_anchored {",
            "pcall(function() h.register_anchored {",
        )
        .replace(
            "interaction = \"increment\",\n    }",
            "interaction = \"increment\",\n    } end)",
        );
    assert_ne!(bad, main);
    package(&fixture, &bad, &behavior, true);
    assert!(
        fixture.open().is_err(),
        "caught invalid registration must still invalidate startup"
    );
    assert!(!fixture.0.join("save").exists());
}

#[test]
fn luau_anchored_invalid_interaction_and_excess_refund_preserve_state_over_real_listener() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let (main, behavior) = sources();
    let behavior = format!(
        "local original = (function()\n{behavior}\nend)()\nreturn function(e) if e.kind == 'Interact' then return string.rep('x', 10) elseif e.kind == 'Refund' then return e.maximum + 1 end return original(e) end"
    );
    package(&fixture, &main, &behavior, true);
    let root = fixture.0.join("packages");
    let mut state = Box::new(open(&fixture, &root));
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let block = catalog.state_by_key("counter:block").unwrap();
    let mut saved_id = None;
    gameplay::serve(state, |address| {
        let (mut peer, mut client) = connect(address, &catalog, &fixture);
        let place = edit(&mut client, FIRST, block, 0);
        assert!(send(&mut peer, &mut client, &catalog, place));
        until(&mut peer, &mut client, &catalog, |c| {
            c.anchored(FIRST).is_some() && c.player_count(0) == 6
        });
        saved_id = Some(client.anchored(FIRST).unwrap().id);
        let interact = client.action_on_block([0, 81, 2]);
        assert!(!send(&mut peer, &mut client, &catalog, interact.clone()));
        assert!(!send(&mut peer, &mut client, &catalog, interact));
        assert_eq!(client.anchored(FIRST).unwrap().payload, [0; 5]);
        let remove = edit(&mut client, [0, 81, 2], crate::world::AIR, 0);
        assert!(!send(&mut peer, &mut client, &catalog, remove.clone()));
        assert!(!send(&mut peer, &mut client, &catalog, remove));
        assert_eq!(client.player_count(0), 6);
        assert_eq!(client.block_state(FIRST), Some(block));
        assert_eq!(client.block_state([0, 81, 2]), Some(block));
        protocol::write_client(&mut peer, &ClientMessage::Ping { nonce: 0xAC03 }).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline);
            let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
            let pong = matches!(message, ServerMessage::Pong { nonce: 0xAC03 });
            client.accept(message);
            if pong {
                break;
            }
        }
        let _ = peer.shutdown(Shutdown::Both);
    });
    let mut recovered = open(&fixture, &root);
    let id = recovered
        .entities
        .anchored_at(CellCoord::new(FIRST[0], FIRST[1], FIRST[2]))
        .unwrap();
    assert_eq!(Some(id.get()), saved_id);
    assert_eq!(counter_total(&mut recovered, FIRST), 6);
    let snapshot = recovered.entities.snapshot(id).unwrap();
    let descriptor = recovered
        .entities
        .types()
        .descriptor(snapshot.entity_type)
        .unwrap();
    assert_eq!(
        descriptor.public_view(&snapshot.private_payload).unwrap(),
        [0; 5]
    );
}

//! Format-2 UI through real bundle download, client aim, authorization and WAL.
use super::*;
use crate::{
    client::PackageActionProbe,
    world::{AIR, GLOWSTONE, SAND, STONE},
};

const PROFILE: u128 = 0x719a;
const FIRST: [i32; 3] = [2, 81, 0];
const SECOND: [i32; 3] = [2, 81, 2];
const THIRD: [i32; 3] = [0, 81, 3];

#[test]
fn authored_block_action_rejects_observed_stone_after_remove_and_restore_over_listener() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let packages = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/ui-target-actions/packages");
    let open = || {
        let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
            .with_local_packages(&packages)
            .unwrap();
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap()
    };
    let mut state = open();
    state.spawn_anchor = [0.5, 80.0, 0.5];
    let target = [3, 81, 0];
    for x in -1..=4 {
        for z in -1..=1 {
            state.world.edit(x, 79, z, STONE).unwrap();
            for y in 80..=83 {
                state.world.edit(x, y, z, AIR).unwrap();
            }
        }
    }
    state
        .world
        .edit(target[0], target[1], target[2], STONE)
        .unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(crate::items::STICK, 2));
    inventory.slots[1] = Some(Stack::new(crate::content::ItemId(STONE.0), 1));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    gameplay::serve(Box::new(state), |address| {
        let mut client = PackageActionProbe::connect(
            &address.to_string(),
            PROFILE,
            fixture.0.join("client-config"),
        );
        client.ready(target, STONE, 0);
        let observed = client.observe(target);
        // Each completion is an explicit durable boundary, not a sleep or a
        // race against the listener. The client retains its old observation.
        let removed = client.edit(target, AIR, 1);
        let result = client.result(&removed);
        assert!(result.0, "{}", result.1);
        let restored = client.edit(target, STONE, 1);
        let result = client.result(&restored);
        assert!(result.0, "{}", result.1);
        client.ready(target, STONE, 1);
        let stale = client.submit_observed(&observed);
        let result = client.result(&stale);
        assert!(!result.0, "stale same-type replacement applied");
        assert!(result.1.contains("target changed"), "{}", result.1);
        assert_eq!(client.count(), 2, "stale action consumed its cost");
        let fresh = client.submit_panel_block(target);
        let result = client.result(&fresh);
        assert!(result.0, "{}", result.1);
        client.ready(target, GLOWSTONE, 2);
        assert_eq!(client.count(), 1);
    });
    let mut recovered = open();
    assert_eq!(
        recovered
            .world
            .get_block(target[0], target[1], target[2])
            .unwrap(),
        GLOWSTONE
    );
    assert_eq!(
        recovered.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
}

#[test]
fn authored_block_action_current_aim_denials_and_reconnect_recovery() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let packages = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/ui-target-actions/packages");
    let open = || {
        let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
            .with_local_packages(&packages)
            .unwrap();
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap()
    };
    let mut old_request = None;
    for restarted in [false, true] {
        let mut state = open();
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if !restarted {
            // Explicit support/headroom makes spawn and sight deterministic.
            for x in -1..=11 {
                for z in -3..=4 {
                    state.world.edit(x, 79, z, STONE).unwrap();
                    for y in 80..=83 {
                        state.world.edit(x, y, z, AIR).unwrap();
                    }
                }
            }
            for [x, y, z] in [FIRST, SECOND, THIRD, [4, 81, 0], [10, 81, 0]] {
                state.world.edit(x, y, z, STONE).unwrap();
            }
            state.world.edit(2, 81, -2, SAND).unwrap();
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::new(
                state
                    .world
                    .catalog()
                    .item_by_key("bloxgloom:stick")
                    .unwrap(),
                4,
            ));
            state.inventory_store.save(PROFILE, &inventory).unwrap();
        } else {
            assert_eq!(state.world.get_block(2, 81, 0).unwrap(), GLOWSTONE);
            assert_eq!(state.world.get_block(2, 81, 2).unwrap(), GLOWSTONE);
            assert_eq!(state.world.get_block(4, 81, 0).unwrap(), STONE);
            assert_eq!(state.world.get_block(10, 81, 0).unwrap(), STONE);
        }
        gameplay::serve(Box::new(state), |address| {
            let mut client = PackageActionProbe::connect(
                &address.to_string(),
                PROFILE,
                fixture.0.join("client-config"),
            );
            // Even with a pending join stream, unloaded client terrain cannot
            // become a procedural-fallback target or consume a session ID.
            if !restarted {
                client.reject_locally(FIRST);
            }
            client.ready(
                if restarted { THIRD } else { FIRST },
                STONE,
                if restarted { 2 } else { 0 },
            );
            assert_eq!(client.count(), if restarted { 2 } else { 4 });
            if restarted {
                client.reject_old_session(old_request.as_ref().unwrap());
                let request = client.click(THIRD, THIRD);
                let result = client.result(&request);
                assert!(result.0, "{}", result.1);
                client.ready(THIRD, GLOWSTONE, 3);
                assert_eq!(client.count(), 1);
                return;
            }
            // No target, and a visible wrong block, must fail before ID allocation.
            client.reject_locally([0, 83, 0]);
            client.reject_locally([2, 81, -2]);
            let request = client.click(SECOND, FIRST);
            let ClientMessage::EntityInteract {
                action_id,
                target,
                payload,
            } = &request
            else {
                unreachable!()
            };
            assert_eq!(*action_id as u64, 1);
            assert_eq!(
                *target, FIRST,
                "dispatch must use current, not click-time aim"
            );
            let observed = bloxgloom_host_api::actions::TerrainRequest::decode(payload).unwrap();
            let decoded = observed.request;
            assert_eq!(
                (
                    decoded.slot,
                    decoded.inventory_revision,
                    decoded.entity,
                    decoded.entity_revision
                ),
                (0, 0, 0, 0)
            );
            assert!(decoded.arguments.is_empty());
            let result = client.result(&request);
            assert!(result.0, "{}", result.1);
            assert_eq!(client.feedback(), "SERVER APPLIED ACTION");
            client.ready(FIRST, GLOWSTONE, 1);
            assert_eq!(client.count(), 3);

            // Forged fields/coordinates never bypass the existing server path.
            for (target, mutation, reason) in [
                (SECOND, 1, "block use has an entity"),
                ([10, 81, 0], 0, "out of reach"),
                ([4, 81, 0], 0, "occluded"),
                (FIRST, 0, "target changed"),
                (SECOND, 2, "stale actor inventory"),
                // Wire reasons are bounded; the handler's full assertion text
                // is not transmitted, but package attribution is retained.
                (SECOND, 3, "uitarget:light:"),
            ] {
                let forged = client.forged(&request, target, |request| match mutation {
                    1 => {
                        request.entity = 99;
                        request.entity_revision = 1;
                    }
                    2 => request.inventory_revision = 0,
                    3 => request.slot = 1,
                    _ => {}
                });
                let result = client.result(&forged);
                assert!(!result.0, "forgery applied: {reason}");
                assert!(result.1.contains(reason), "expected {reason}: {}", result.1);
                assert!(client.feedback().starts_with("SERVER DENIED:"));
                assert_eq!(client.count(), 3, "denial consumed cost");
            }
            // Local changed-target rejection and subsequent valid retry stay live.
            client.reject_locally(FIRST);
            let retry = client.click(FIRST, SECOND);
            let result = client.result(&retry);
            assert!(result.0, "{}", result.1);
            client.ready(SECOND, GLOWSTONE, 2);
            assert_eq!(client.count(), 2);
            old_request = Some(request);
        });
    }
    let mut recovered = open();
    assert_eq!(recovered.world.get_block(0, 81, 3).unwrap(), GLOWSTONE);
    // The last reconnect's cost and terrain edit survive another WAL recovery.
    assert_eq!(
        recovered.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
}

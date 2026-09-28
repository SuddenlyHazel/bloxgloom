use super::*;

#[test]
fn mod_admin_grant_requires_server_identity_and_replays_once_over_listener() {
    let fixture = Fixture::new();
    fixture.action(
        &REGISTER.replace("'item', 'bloxgloom:stick'", "'empty', nil"),
        "return function(c,e) assert(c.admin_give('bloxgloom:stick',1)) end",
    );
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        let (accepted, reason) = peer.send(&request);
        assert!(!accepted, "{reason}");
        assert!(reason.contains("admin access denied"), "{reason}");
        assert_eq!(peer.inventory.slots[0], None);
    });
    let mut state = Box::new(fixture.open().unwrap());
    state.admin_profile = Some(PROFILE);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        assert!(peer.send(&request).0);
        assert!(peer.send(&request).0, "receipt retry must not grant again");
        peer.inventory_at(1);
    });
    let mut state = Box::new(fixture.open().unwrap());
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0],
        Some(Stack::new(crate::items::STICK, 1))
    );
    state.admin_profile = Some(PROFILE);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        peer.inventory_at(1);
        let request = peer.request(0);
        assert!(peer.send(&request).0);
        peer.inventory_at(2);
    });
}

#[test]
fn mod_admin_spawn_and_drop_share_one_allocator_and_restart() {
    let fixture = Fixture::new();
    fixture.action(
        &REGISTER.replace("'item', 'bloxgloom:stick'", "'empty', nil"),
        "return function(c,e) c.spawn_drop(4,80,0,'bloxgloom:stick',1,1500); if string.byte(e.arguments,1) == 1 then c.set_block(2,80,0,'bloxgloom:stone') end; c.admin_spawn('bloxgloom:mossbun') end",
    );
    let mut state = Box::new(fixture.open().unwrap());
    state.admin_profile = Some(PROFILE);
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -5..=5 {
        for z in -5..=5 {
            for y in 79..=83 {
                state
                    .world
                    .edit(x, y, z, if y == 79 { crate::world::STONE } else { AIR })
                    .unwrap();
            }
        }
    }
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let conflict = peer.request(1);
        assert!(
            !peer.send(&conflict).0,
            "spawn must not validate pre-edit ground"
        );
        let request = peer.request(0);
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        assert!(peer.send(&request).0);
    });
    let mut state = fixture.open().unwrap();
    assert_eq!(state.world.get_block(2, 80, 0).unwrap(), AIR);
    let views = state
        .entities
        .public_views_for_chunk_bounded(crate::world::world_to_chunk(2, 80, 0).0, 32)
        .unwrap();
    assert_eq!(
        views
            .iter()
            .filter(|view| view.entity_type == crate::content::MOSSBUN_ENTITY_TYPE)
            .count(),
        1
    );
    assert_eq!(
        views
            .iter()
            .filter(|view| view.entity_type == crate::server::drops::DROP_ENTITY_TYPE)
            .count(),
        1
    );
    assert_ne!(views[0].id, views[1].id);
}

#[test]
fn luau_action_block_targets_keep_real_reach_sight_and_identity_checks() {
    let fixture = Fixture::new();
    fixture.action(
        &REGISTER.replace("'item', 'bloxgloom:stick'", "'block', 'bloxgloom:stone'"),
        r#"return function(c,e)
            assert(e.cell[1] == 2 and e.cell[2] == 80 and e.cell[3] == 0)
            assert(c.block(2,80,0).block_type == 'bloxgloom:stone')
            c.set_block(2,80,0,'bloxgloom:glowstone')
        end"#,
    );
    let mut state = Box::new(fixture.open().unwrap());
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for (x, y, z, block) in [
        (0, 79, 0, crate::world::STONE),
        (0, 80, 0, AIR),
        (0, 81, 0, AIR),
        (1, 80, 0, AIR),
        (1, 81, 0, AIR),
        (2, 81, 0, AIR),
        (2, 80, 0, crate::world::STONE),
        (2, 80, 2, AIR),
        (3, 80, 0, crate::world::STONE),
        (3, 81, 0, crate::world::STONE),
        (4, 80, 0, crate::world::STONE),
        (10, 80, 0, crate::world::STONE),
        (64, 80, 0, crate::world::STONE),
    ] {
        state.world.edit(x, y, z, block).unwrap();
    }
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        for (target, expected_reason) in [
            ([64, 80, 0], "interest"),
            ([10, 80, 0], "reach"),
            ([2, 80, 2], "changed"),
            ([4, 80, 0], "occluded"),
        ] {
            let mut request = peer.request(0);
            if let ClientMessage::EntityInteract { target: at, .. } = &mut request {
                *at = target;
            }
            let (accepted, reason) = peer.send(&request);
            assert!(!accepted, "{reason}");
            assert!(reason.contains(expected_reason), "{reason}");
        }
        let mut request = peer.request(0);
        if let ClientMessage::EntityInteract { target, .. } = &mut request {
            *target = [2, 80, 0];
        }
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        // A fresh action cannot re-use a target whose authoritative type changed.
        let mut request = peer.request(0);
        if let ClientMessage::EntityInteract { target, .. } = &mut request {
            *target = [2, 80, 0];
        }
        assert!(!peer.send(&request).0);
    });
    let mut recovered = fixture.open().unwrap();
    assert_eq!(recovered.world.get_block(2, 80, 0).unwrap(), GLOWSTONE);
    assert_eq!(
        recovered.world.get_block(4, 80, 0).unwrap(),
        crate::world::STONE
    );
    assert_eq!(
        recovered.world.get_block(64, 80, 0).unwrap(),
        crate::world::STONE
    );
}

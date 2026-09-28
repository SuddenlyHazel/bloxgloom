use super::*;

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

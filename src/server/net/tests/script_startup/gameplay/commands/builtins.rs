use super::*;

#[test]
fn advertised_builtin_commands_and_compatibility_packets_share_auth_receipts_and_restart() {
    let fixture = Fixture::new();
    let mut old = None;
    for round in 0..3 {
        let mut state = Box::new(fixture.open().unwrap());
        state.admin_profile = (round != 0).then_some(PROFILE);
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if round == 0 {
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
        }
        let catalog = state.world.catalog_arc();
        let give = catalog
            .action(crate::gameplay::admin::GIVE)
            .unwrap()
            .command
            .clone()
            .unwrap();
        let spawn = catalog
            .action(crate::gameplay::admin::SPAWN)
            .unwrap()
            .command
            .clone()
            .unwrap();
        assert_eq!(give.permission, CommandPermission::Admin);
        assert_eq!(spawn.permission, CommandPermission::Admin);
        let mut grants = if round == 0 { 0 } else { (round - 1) * 2 };
        serve(state, |address| {
            let mut peer = Peer::connect(address, catalog);
            if let Some(old) = &old {
                assert!(!peer.send(old).0, "old-session command cannot replay");
            }
            for legacy in [false, true] {
                let request = typed_request(
                    &mut peer,
                    crate::gameplay::admin::GIVE,
                    give.encode_arguments(&["bloxgloom:stick", "1"]).unwrap(),
                );
                let request = if legacy {
                    let ClientMessage::EntityInteract { action_id, .. } = request else {
                        unreachable!()
                    };
                    ClientMessage::AdminGive {
                        action_id,
                        item: crate::items::STICK,
                        count: 1,
                    }
                } else {
                    request
                };
                let (accepted, reason) = peer.send(&request);
                assert_eq!(accepted, round != 0, "{reason}");
                if accepted {
                    grants += 1;
                    peer.inventory_at(grants);
                    assert!(peer.send(&request).0, "grant receipt replay");
                } else {
                    assert!(reason.contains("requires admin"), "{reason}");
                    assert!(!peer.send(&request).0);
                }
                old = Some(request);
                let request = typed_request(
                    &mut peer,
                    crate::gameplay::admin::SPAWN,
                    spawn.encode_arguments(&["bloxgloom:mossbun"]).unwrap(),
                );
                let request = if legacy {
                    let ClientMessage::EntityInteract { action_id, .. } = request else {
                        unreachable!()
                    };
                    ClientMessage::AdminSpawnEntity {
                        action_id,
                        entity_type: crate::content::MOSSBUN_ENTITY_TYPE,
                    }
                } else {
                    request
                };
                let (accepted, reason) = peer.send(&request);
                assert_eq!(accepted, round != 0, "{reason}");
                assert_eq!(peer.send(&request).0, accepted, "spawn receipt replay");
                if !accepted {
                    assert!(reason.contains("requires admin"), "{reason}");
                }
            }
        });
        let recovered = fixture.open().unwrap();
        assert_eq!(
            recovered.inventory_store.load(PROFILE).unwrap().slots[0],
            (grants != 0).then(|| Stack::new(crate::items::STICK, grants))
        );
        let mut creatures = 0;
        for x in -1..=0 {
            for z in -1..=0 {
                creatures += recovered
                    .entities
                    .public_views_for_chunk_bounded(crate::world::ChunkKey { x, y: 5, z }, 32)
                    .unwrap()
                    .iter()
                    .filter(|view| view.entity_type == crate::content::MOSSBUN_ENTITY_TYPE)
                    .count();
            }
        }
        assert_eq!(
            creatures,
            usize::from(grants),
            "spawn and receipt recovery must be exact"
        );
    }
}

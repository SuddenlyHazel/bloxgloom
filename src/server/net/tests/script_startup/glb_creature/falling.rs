//! Support removal through real edits, server physics and native replication.
use super::*;
use crate::protocol::PublicEntityLocation;

#[test]
fn packaged_creature_falls_after_breaking_its_platform_over_real_listener() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let packages =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/glb-creatures/packages");
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&packages)
        .unwrap();
    let mut state = Box::new(
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap(),
    );
    state.admin_profile = Some(0x5c71);
    state.spawn_anchor = [0.5, 85.0, 0.5];
    let catalog = state.world.catalog_arc();
    let kind = catalog.entity_type_id_by_key("sprout:sproutling").unwrap();
    for x in -3..=3 {
        for z in -3..=3 {
            for y in 79..=88 {
                state
                    .world
                    .edit(
                        x,
                        y,
                        z,
                        if y == 79 || y == 84 {
                            crate::world::STONE
                        } else {
                            crate::world::AIR
                        },
                    )
                    .unwrap();
            }
        }
    }
    gameplay::serve(state, |address| {
        let mut admin = gameplay::Peer::connect(address, Arc::clone(&catalog));
        let spawn = ClientMessage::AdminSpawnEntity {
            action_id: admin.next_id(),
            entity_type: kind,
        };
        let (accepted, reason) = admin.send(&spawn);
        assert!(accepted, "{reason}");
        let mut probe = NetworkedVisualProbe::connect(
            &address.to_string(),
            0xB449,
            fixture.0.join("fall-client"),
        )
        .unwrap();
        let first = joined(&mut probe, kind);
        let PublicEntityLocation::Mobile { position } = first.location else {
            panic!("creature must be mobile");
        };
        assert_eq!(position[1], 85.0);
        let x = position[0].floor() as i32;
        let z = position[2].floor() as i32;
        // Remove the entire footprint despite the creature's small patrol.
        for bx in x - 1..=x + 1 {
            for bz in z - 1..=z + 1 {
                let edit = ClientMessage::Edit {
                    action_id: admin.next_id(),
                    x: bx,
                    y: 84,
                    z: bz,
                    block: crate::world::AIR,
                    slot: 0,
                };
                let (accepted, reason) = admin.send(&edit);
                assert!(accepted, "support edit rejected: {reason}");
            }
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut fall_started = None;
        loop {
            let entity = probe.entity(kind).unwrap();
            let PublicEntityLocation::Mobile { position } = entity.location else {
                panic!("creature must remain mobile");
            };
            let visual = probe.model_pose(entity.id).unwrap();
            if position[1] < 85.0 {
                let started = *fall_started.get_or_insert(visual.sample_tick);
                assert!(
                    visual.sample_tick - started < 50,
                    "slow script callbacks must not stretch a five-block fall past a second: elapsed={} ticks, height={}",
                    visual.sample_tick - started,
                    position[1]
                );
                if position[1] == 80.0 {
                    assert!(
                        catalog
                            .mobile_entity(kind)
                            .unwrap()
                            .behavior
                            .pose(&entity.payload)
                            .unwrap()
                            .grounded
                    );
                    break;
                }
            }
            assert!(Instant::now() < deadline, "creature did not land");
            probe.accept_next();
        }
    });
}

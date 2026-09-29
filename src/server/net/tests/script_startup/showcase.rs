//! One reusable authored package across server join, mobile replica and client worker.
use super::*;
use crate::client::{InventoryProbe, NetworkedVisualProbe};
use crate::server::startup::ServerStartup;
use std::sync::Arc;
use std::time::Instant;

const PRESS_ANCHOR: [i32; 3] = [2, 79, 2];

fn send_action(
    peer: &mut TcpStream,
    admin: &mut InventoryProbe,
    catalog: &Catalog,
    action_id: u128,
    message: ClientMessage,
) {
    protocol::write_client_with_catalog(&mut *peer, &message, catalog).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "showcase action deadline");
        let message = protocol::read_server_with_catalog(&mut *peer, catalog).unwrap();
        let accepted = match &message {
            ServerMessage::ActionResult {
                action_id: result,
                accepted,
                reason,
            } if *result == action_id => {
                assert!(*accepted, "{reason}");
                true
            }
            _ => false,
        };
        admin.accept(message);
        if accepted {
            break;
        }
    }
}

#[test]
fn phase4_showcase_creature_machine_and_replica_survive_real_join_and_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let packages =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/phase4-showcase/packages");
    let mut saved_id = None;
    let mut saved_machine_id = None;
    let mut fingerprint = None;
    for restarted in [false, true] {
        let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
            .with_local_packages(&packages)
            .unwrap();
        let mut state = Box::new(
            crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup)
                .unwrap(),
        );
        state.admin_profile = Some(0xA440);
        let catalog = state.world.catalog_arc();
        let creature = catalog.entity_type_id_by_key("demo:sproutling").unwrap();
        let machine = catalog.entity_type_id_by_key("demo:press_machine").unwrap();
        let press = catalog.state_by_key("demo:press[lit=off]").unwrap();
        let lit_press = catalog.state_by_key("demo:press[lit=on]").unwrap();
        assert_eq!(catalog.machine(machine).unwrap().variants[0].idle.len(), 2);
        assert_eq!(
            catalog.inventory_screen(machine).unwrap().title,
            "STONE PRESS"
        );
        assert_eq!(catalog.mobile_entity(creature).unwrap().model.len(), 5);
        if let Some(expected) = fingerprint {
            assert_eq!(catalog.fingerprint(), expected);
        } else {
            fingerprint = Some(catalog.fingerprint());
        }
        if !restarted {
            state.spawn_anchor = [0.5, 79.0, 0.5];
            for x in -3..=3 {
                for z in -3..=3 {
                    for y in 78..=83 {
                        state
                            .world
                            .edit(
                                x,
                                y,
                                z,
                                if y == 78 {
                                    crate::world::STONE
                                } else {
                                    crate::world::AIR
                                },
                            )
                            .unwrap();
                    }
                }
            }
            let mut inventory = crate::inventory::Inventory::default();
            inventory.slots[0] = Some(crate::inventory::Stack::new(
                catalog.item_by_key("demo:press").unwrap(),
                1,
            ));
            inventory.slots[1] = Some(crate::inventory::Stack::new(
                catalog.item_by_key("bloxgloom:stone").unwrap(),
                1,
            ));
            inventory.slots[2] = Some(crate::inventory::Stack::new(crate::items::STICK, 1));
            state.inventory_store.save(0xA440, &inventory).unwrap();
        } else {
            let id = crate::server::entities::EntityId::new(saved_id.unwrap()).unwrap();
            let snapshot = state.entities.snapshot(id).unwrap();
            let bytes = catalog
                .mobile_entity(creature)
                .unwrap()
                .behavior
                .encode(&snapshot.private_payload)
                .unwrap();
            assert_eq!(&bytes[12..], b"happy");
            let inventory = state.inventory_store.load(0xA440).unwrap();
            assert!(inventory.slots[..3].iter().all(Option::is_none));
            for [x, z] in [[-2, 2], [-2, 1], [-1, 2], [-1, 1]] {
                assert_eq!(
                    state.world.get_block(x, 79, z).unwrap(),
                    crate::world::STONE
                );
            }
            let saved_press = state
                .world
                .get_block(PRESS_ANCHOR[0], PRESS_ANCHOR[1], PRESS_ANCHOR[2])
                .unwrap();
            assert!(saved_press == press || saved_press == lit_press);
            assert_eq!(
                state
                    .world
                    .get_block(PRESS_ANCHOR[0] + 1, PRESS_ANCHOR[1], PRESS_ANCHOR[2])
                    .unwrap(),
                saved_press
            );
            let anchor_id = state
                .entities
                .anchored_at(crate::server::entities::CellCoord::new(
                    PRESS_ANCHOR[0],
                    PRESS_ANCHOR[1],
                    PRESS_ANCHOR[2],
                ));
            assert!(anchor_id.is_some());
            assert_eq!(
                state
                    .entities
                    .anchored_at(crate::server::entities::CellCoord::new(
                        PRESS_ANCHOR[0] + 1,
                        PRESS_ANCHOR[1],
                        PRESS_ANCHOR[2],
                    )),
                anchor_id
            );
        }
        gameplay::serve(state, |address| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_nodelay(true).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "showcase-admin".into(),
                    profile: 0xA440,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut peer).unwrap()
            else {
                panic!("expected showcase bundle offer");
            };
            crate::client::bundle::receive(&mut peer, identity, None).unwrap();
            let (joined_fingerprint, _) = receive_content_manifest(&mut peer);
            assert_eq!(joined_fingerprint, catalog.fingerprint());
            protocol::write_client(
                &mut peer,
                &ClientMessage::ContentReady {
                    fingerprint: joined_fingerprint,
                },
            )
            .unwrap();
            let mut admin = InventoryProbe::new(
                catalog.clone(),
                fixture.0.join(format!("showcase-admin-{restarted}")),
            );
            loop {
                let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
                let ready = matches!(message, ServerMessage::ActionSession { .. });
                admin.accept(message);
                if ready {
                    break;
                }
            }
            let mut visual = NetworkedVisualProbe::connect(
                &address.to_string(),
                0xA441,
                fixture.0.join(format!("showcase-visual-{restarted}")),
            )
            .unwrap();
            if !restarted {
                let action_id = admin.next_id();
                send_action(
                    &mut peer,
                    &mut admin,
                    &catalog,
                    action_id,
                    ClientMessage::AdminSpawnEntity {
                        action_id,
                        entity_type: creature,
                    },
                );
                let action_id = admin.next_id();
                send_action(
                    &mut peer,
                    &mut admin,
                    &catalog,
                    action_id,
                    ClientMessage::Edit {
                        action_id,
                        x: PRESS_ANCHOR[0],
                        y: PRESS_ANCHOR[1],
                        z: PRESS_ANCHOR[2],
                        block: press,
                        slot: 0,
                    },
                );
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while admin.anchored(PRESS_ANCHOR).is_none() {
                assert!(Instant::now() < deadline, "showcase press replica deadline");
                admin.accept(protocol::read_server_with_catalog(&mut peer, &catalog).unwrap());
            }
            if !restarted {
                let deadline = Instant::now() + Duration::from_secs(10);
                while admin.player_count(0) != 0 {
                    assert!(
                        Instant::now() < deadline,
                        "showcase placement inventory deadline"
                    );
                    admin.accept(protocol::read_server_with_catalog(&mut peer, &catalog).unwrap());
                }
            }
            let secondary = [PRESS_ANCHOR[0] + 1, PRESS_ANCHOR[1], PRESS_ANCHOR[2]];
            let current_state = admin.block_state(secondary).unwrap();
            admin.open(secondary, current_state);
            if !restarted {
                for (player, container) in [(2, 0), (1, 1)] {
                    let request = admin.transfer(true, player, container, true);
                    let action_id = match &request {
                        ClientMessage::EntityInteract { action_id, .. } => *action_id,
                        _ => unreachable!("inventory probe transfer"),
                    };
                    send_action(&mut peer, &mut admin, &catalog, action_id, request);
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while admin.player_count(player as usize) != 0
                        || admin.view().unwrap().slots[container as usize]
                            .as_ref()
                            .map(|stack| stack.count)
                            != Some(1)
                    {
                        assert!(
                            Instant::now() < deadline,
                            "showcase transfer replica deadline"
                        );
                        admin.accept(
                            protocol::read_server_with_catalog(&mut peer, &catalog).unwrap(),
                        );
                    }
                }
                let stone = catalog.item_by_key("bloxgloom:stone").unwrap();
                let action_id = admin.next_id();
                send_action(
                    &mut peer,
                    &mut admin,
                    &catalog,
                    action_id,
                    ClientMessage::AdminGive {
                        action_id,
                        item: stone,
                        count: 4,
                    },
                );
                let deadline = Instant::now() + Duration::from_secs(10);
                while admin.player_count(0) != 4
                    || !admin.ready(crate::world::world_to_chunk(-2, 79, 2).0)
                {
                    assert!(
                        Instant::now() < deadline,
                        "showcase edit readiness deadline"
                    );
                    admin.accept(protocol::read_server_with_catalog(&mut peer, &catalog).unwrap());
                }
                let mut response = Vec::new();
                for (index, [x, z]) in [[-2, 2], [-2, 1], [-1, 2], [-1, 1]].into_iter().enumerate()
                {
                    let action_id = admin.next_id();
                    let start = Instant::now();
                    send_action(
                        &mut peer,
                        &mut admin,
                        &catalog,
                        action_id,
                        ClientMessage::Edit {
                            action_id,
                            x,
                            y: 79,
                            z,
                            block: crate::world::STONE,
                            slot: 0,
                        },
                    );
                    response.push(start.elapsed());
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while admin.player_count(0) != 3 - index as u16
                        || admin.block_state([x, 79, z]) != Some(crate::world::STONE)
                    {
                        assert!(
                            Instant::now() < deadline,
                            "showcase edit replica deadline: cell=({x},{z}) count={} state={:?}",
                            admin.player_count(0),
                            admin.block_state([x, 79, z])
                        );
                        admin.accept(
                            protocol::read_server_with_catalog(&mut peer, &catalog).unwrap(),
                        );
                    }
                }
                response.sort();
                eprintln!(
                    "showcase mixed edit response p50={:?} p95={:?}",
                    response[2], response[3]
                );
                let deadline = Instant::now() + Duration::from_secs(10);
                while admin.view().unwrap().slots[2]
                    .as_ref()
                    .map(|stack| stack.count)
                    != Some(2)
                {
                    assert!(Instant::now() < deadline, "showcase process deadline");
                    admin.accept(protocol::read_server_with_catalog(&mut peer, &catalog).unwrap());
                }
            }
            assert_eq!(
                admin.view().unwrap().slots[2].as_ref().unwrap(),
                &crate::inventory::Stack::new(catalog.item_by_key("bloxgloom:gravel").unwrap(), 2,)
            );
            admin.close();
            let deadline = Instant::now() + Duration::from_secs(10);
            while visual
                .entity(creature)
                .is_none_or(|entity| entity.revision <= 1)
            {
                assert!(Instant::now() < deadline, "showcase replica deadline");
                visual.accept_next();
            }
            let entity = visual.entity(creature).unwrap();
            assert_eq!(entity.payload.len(), 5);
            let deadline = Instant::now() + Duration::from_secs(10);
            while !visual
                .offered_anchors()
                .0
                .iter()
                .any(|view| view.key == "demo:press_machine")
            {
                assert!(
                    Instant::now() < deadline,
                    "showcase anchor replica deadline"
                );
                visual.accept_next();
            }
            let machine_id = visual
                .offered_anchors()
                .0
                .into_iter()
                .find(|view| view.key == "demo:press_machine")
                .unwrap()
                .id;
            if let Some(expected) = saved_machine_id {
                assert_eq!(machine_id, expected);
            } else {
                saved_machine_id = Some(machine_id);
            }
            let (offered, total) = visual.offered();
            assert!(total >= 1, "visual owner did not receive the entity");
            assert!(offered.iter().any(|view| view.id == entity.id));
            if let Some(expected) = saved_id {
                assert_eq!(entity.id, expected);
            } else {
                saved_id = Some(entity.id);
            }
            let expected_tint = if entity.payload[4] == 1 {
                [0.7, 1.0, 0.7]
            } else {
                [1.0, 1.0, 0.5]
            };
            for _ in 0..32 {
                visual.settle();
                if visual.tint(entity.id) == Some(expected_tint)
                    && visual.has_anchor_spark(machine_id)
                {
                    break;
                }
            }
            assert_eq!(visual.tint(entity.id), Some(expected_tint));
            assert!(visual.has_spark(entity.id));
            assert!(visual.has_anchor_spark(machine_id));
            if !restarted {
                let before = entity.revision;
                assert!(
                    visual.interact(&entity),
                    "authored creature interaction failed"
                );
                let deadline = Instant::now() + Duration::from_secs(10);
                while visual
                    .entity(creature)
                    .is_none_or(|current| current.revision <= before)
                {
                    assert!(Instant::now() < deadline, "interaction replica deadline");
                    visual.accept_next();
                }
            }
        });
    }
}

#[test]
fn local_package_showcase_joins_and_restarts_through_nonblocking_listener() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let packages =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/phase4-showcase/packages");
    let save = fixture.0.join("local-package-save");
    let mut fingerprint = None;
    for _ in 0..2 {
        let (address, server) =
            crate::server::start_local_server_with_packages(7, save.clone(), 0xA440, &packages)
                .unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let catalog =
                crate::client::connect_catalog_probe(&address.to_string(), 0xA441).unwrap();
            assert!(
                catalog
                    .entity_type_id_by_key("demo:press_machine")
                    .is_some()
            );
            if let Some(expected) = fingerprint {
                assert_eq!(catalog.fingerprint(), expected);
            } else {
                fingerprint = Some(catalog.fingerprint());
            }
            crate::client::connect_visual_probe(&address.to_string(), 0xA442, |visual| {
                assert_eq!(visual.owner(), "demo");
            })
            .unwrap();
        }));
        server.stop().unwrap();
        result.unwrap();
    }
}

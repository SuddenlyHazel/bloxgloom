//! Full external registration -> listener -> worker -> WAL -> replica -> click path.
use super::*;
use crate::{client::MobileProbe, server::startup::ServerStartup};
use std::{sync::Arc, time::Instant};
fn until(
    peer: &mut TcpStream,
    client: &mut MobileProbe,
    catalog: &crate::content::Catalog,
    ready: impl Fn(&MobileProbe) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready(client) {
        assert!(Instant::now() < deadline);
        client.accept(protocol::read_server_with_catalog(&mut *peer, catalog).unwrap());
    }
}
fn send(
    peer: &mut TcpStream,
    client: &mut MobileProbe,
    catalog: &crate::content::Catalog,
    request: &ClientMessage,
) -> bool {
    let id = match request {
        ClientMessage::AdminSpawnEntity { action_id, .. }
        | ClientMessage::Edit { action_id, .. }
        | ClientMessage::AdminGive { action_id, .. }
        | ClientMessage::EntityInteract { action_id, .. } => *action_id,
        _ => panic!(),
    };
    protocol::write_client_with_catalog(&mut *peer, request, catalog).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline);
        let message = protocol::read_server_with_catalog(&mut *peer, catalog).unwrap();
        client.accept(message.clone());
        if let ServerMessage::ActionResult {
            action_id,
            accepted,
            ..
        } = message
            && action_id == id
        {
            return accepted;
        }
    }
}
#[test]
fn external_creature_spawns_moves_targets_interacts_and_recovers_over_real_listener() {
    creature_probe(false);
}

#[test]
fn mixed_response_path_keeps_edits_creatures_and_machine_progressing_across_restart() {
    creature_probe(true);
}

fn creature_probe(mixed: bool) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-creature-tcp-{}-{stamp}",
        std::process::id()
    ));
    let mut saved_id = None;
    for restarted in [false, true] {
        let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&bloxgloom_lifecycle_fixture::Fixture)
            .unwrap();
        let mut state = Box::new(
            crate::server::server_state_with_startup(7, save.clone(), 8, startup).unwrap(),
        );
        let catalog = state.world.catalog_arc();
        let entity_type = catalog
            .entity_type_id_by_key(bloxgloom_lifecycle_fixture::creature::KEY)
            .unwrap();
        state.admin_profile = Some(0xFACE);
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if !restarted {
            for x in -5..=8 {
                for z in -5..=8 {
                    for y in 79..=83 {
                        state
                            .world
                            .edit(
                                x,
                                y,
                                z,
                                if y == 79 {
                                    crate::world::STONE
                                } else {
                                    crate::world::AIR
                                },
                            )
                            .unwrap();
                    }
                }
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stop_rx));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_nodelay(true).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "external-creature".into(),
                    profile: 0xFACE,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut peer);
            assert_eq!(fingerprint, catalog.fingerprint());
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let mut client = MobileProbe::new(catalog.clone(), save.join("unused-creature-config"));
            loop {
                let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
                let ready = matches!(message, ServerMessage::ActionSession { .. });
                client.accept(message);
                if ready {
                    break;
                }
            }
            if !restarted {
                if mixed {
                    response_samples(&mut peer, &mut client, &catalog, "baseline");
                }
                let action_id = client.next_id();
                let request = ClientMessage::AdminSpawnEntity {
                    action_id,
                    entity_type,
                };
                assert!(send(&mut peer, &mut client, &catalog, &request));
                assert!(send(&mut peer, &mut client, &catalog, &request));
                until(&mut peer, &mut client, &catalog, |c| {
                    c.entity(entity_type).is_some()
                });
                let first = client.entity(entity_type).unwrap();
                saved_id = Some(first.id);
                until(&mut peer, &mut client, &catalog, |c| {
                    c.entity(entity_type).unwrap().location != first.location
                });
                let entity = client.entity(entity_type).unwrap();
                let mut future = client.interact(&entity);
                if let ClientMessage::EntityInteract { payload, .. } = &mut future {
                    let mut request =
                        bloxgloom_host_api::actions::Request::decode(payload).unwrap();
                    request.entity_revision = u64::MAX;
                    *payload = request.encode().unwrap();
                }
                assert!(!send(&mut peer, &mut client, &catalog, &future));
                let mut accepted = false;
                for _ in 0..8 {
                    let entity = client.entity(entity_type).unwrap();
                    let request = client.interact(&entity);
                    // Real clicks arrive after animation/movement has advanced.
                    // Keep this exact request while replicas advance, then send
                    // it; a movement revision must not make own-state use inert.
                    until(&mut peer, &mut client, &catalog, |c| {
                        c.entity(entity_type).unwrap().revision >= entity.revision + 3
                    });
                    if send(&mut peer, &mut client, &catalog, &request) {
                        assert_eq!(client.status(), Some("Interaction applied"));
                        assert!(send(&mut peer, &mut client, &catalog, &request));
                        accepted = true;
                        break;
                    }
                }
                assert!(accepted, "no revision-fenced interaction succeeded");
                until(&mut peer, &mut client, &catalog, |c| {
                    c.entity(entity_type).unwrap().payload[1] == 1
                });
                let request = ClientMessage::AdminSpawnEntity {
                    action_id: client.next_id(),
                    entity_type,
                };
                assert!(send(&mut peer, &mut client, &catalog, &request));
            } else {
                until(&mut peer, &mut client, &catalog, |c| {
                    c.entity(entity_type).is_some()
                });
                let entity = client.entity(entity_type).unwrap();
                assert_eq!(Some(entity.id), saved_id);
                assert_eq!(entity.payload[1], 1, "pause interaction must persist");
            }
            let stone = crate::items::ItemId(crate::world::STONE.0);
            let grant = ClientMessage::AdminGive {
                action_id: client.next_id(),
                item: stone,
                count: 2,
            };
            assert!(send(&mut peer, &mut client, &catalog, &grant));
            until(&mut peer, &mut client, &catalog, |c| {
                c.item_slot(stone).is_some()
            });
            let request = ClientMessage::Edit {
                action_id: client.next_id(),
                x: 0,
                y: 79,
                z: if restarted { 2 } else { 1 },
                block: crate::world::AIR,
                slot: 0,
            };
            assert!(
                send(&mut peer, &mut client, &catalog, &request),
                "paused creature must not starve nearby edits (restart={restarted})"
            );
            let place = ClientMessage::Edit {
                action_id: client.next_id(),
                x: 0,
                y: 79,
                z: if restarted { 2 } else { 1 },
                block: crate::world::STONE,
                slot: client.item_slot(stone).unwrap(),
            };
            assert!(
                send(&mut peer, &mut client, &catalog, &place),
                "creatures must not starve placement (restart={restarted})"
            );
            if mixed {
                mixed_work(&mut peer, &mut client, &catalog, restarted);
            }
            let _ = peer.shutdown(Shutdown::Both);
        }));
        stop_tx.send(()).unwrap();
        server.join().unwrap().unwrap();
        if let Err(panic) = result {
            let _ = std::fs::remove_dir_all(&save);
            std::panic::resume_unwind(panic);
        }
    }
    std::fs::remove_dir_all(save).unwrap();
}

fn mixed_work(
    peer: &mut TcpStream,
    client: &mut MobileProbe,
    catalog: &crate::content::Catalog,
    restarted: bool,
) {
    let cell = [-2, 80, 1];
    if !restarted {
        for (key, count) in [
            ("fixture:crusher", 1),
            ("bloxgloom:stone", 8),
            ("bloxgloom:stick", 1),
        ] {
            let item = catalog.items().find(|i| i.key == key).unwrap().id;
            let request = ClientMessage::AdminGive {
                action_id: client.next_id(),
                item,
                count,
            };
            assert!(send(peer, client, catalog, &request));
            until(peer, client, catalog, |c| c.item_slot(item).is_some());
        }
        let item = catalog
            .items()
            .find(|i| i.key == "fixture:crusher")
            .unwrap();
        let place = ClientMessage::Edit {
            action_id: client.next_id(),
            x: cell[0],
            y: cell[1],
            z: cell[2],
            block: item.placeable.unwrap(),
            slot: client.item_slot(item.id).unwrap(),
        };
        assert!(send(peer, client, catalog, &place));
        until(peer, client, catalog, |c| c.workstation(cell).is_some());
        for (key, slot, count) in [("bloxgloom:stone", 1, 4u16), ("bloxgloom:stick", 0, 1u16)] {
            let item = catalog.items().find(|i| i.key == key).unwrap().id;
            let mut accepted = false;
            for _ in 0..8 {
                let entity = client.workstation(cell).unwrap();
                let mut payload = vec![2, 0, slot, client.item_slot(item).unwrap()];
                payload.extend(count.to_le_bytes());
                payload.extend(entity.id.to_le_bytes());
                payload.extend(entity.revision.to_le_bytes());
                let request = ClientMessage::EntityInteract {
                    action_id: client.next_id(),
                    target: cell,
                    payload,
                };
                if send(peer, client, catalog, &request) {
                    accepted = true;
                    break;
                }
            }
            assert!(
                accepted,
                "machine input could not acquire a current fenced revision"
            );
        }
    }
    response_samples(
        peer,
        client,
        catalog,
        if restarted { "mixed-restart" } else { "mixed" },
    );
    until(peer, client, catalog, |c| {
        c.workstation(cell)
            .and_then(|e| crate::protocol::workstation::WorkstationView::decode(&e.payload))
            .is_some_and(|view| {
                view.slots
                    .iter()
                    .flatten()
                    .any(|s| s.item == crate::items::ItemId(crate::world::GRAVEL.0) && s.count == 8)
            })
    });
}

fn response_samples(
    peer: &mut TcpStream,
    client: &mut MobileProbe,
    catalog: &crate::content::Catalog,
    phase: &str,
) {
    let stone = crate::items::ItemId(crate::world::STONE.0);
    let grant = ClientMessage::AdminGive {
        action_id: client.next_id(),
        item: stone,
        count: 32,
    };
    assert!(send(peer, client, catalog, &grant));
    until(peer, client, catalog, |c| c.item_slot(stone).is_some());
    let mut samples = Vec::new();
    for bounced in [false, true] {
        for x in [-1, 1] {
            until(peer, client, catalog, |c| {
                c.has_chunk(crate::world::world_to_chunk(x, 80, 3).0)
            });
            for place in [true, false, true, false] {
                let action_id = client.next_id();
                protocol::write_client_with_catalog(
                    &mut *peer,
                    &ClientMessage::Move {
                        seq: action_id as u64,
                        dx: if place { 0.01 } else { -0.01 },
                        dy: 0.0,
                        dz: 0.0,
                    },
                    catalog,
                )
                .unwrap();
                let request = ClientMessage::Edit {
                    action_id,
                    x,
                    y: 80,
                    z: 3,
                    block: if place {
                        crate::world::STONE
                    } else {
                        crate::world::AIR
                    },
                    slot: client.item_slot(stone).unwrap(),
                };
                let start = Instant::now();
                assert!(send(peer, client, catalog, &request));
                let confirmed = start.elapsed();
                let mesh = client.mesh_edit(crate::world::world_to_chunk(x, 80, 3).0, bounced);
                samples.push((confirmed, mesh));
                eprintln!(
                    "response phase={phase} bounced={bounced} x={x} place={place}: confirmed={confirmed:?}, worker_mesh={mesh:?}, through_mesh={:?}",
                    start.elapsed()
                );
            }
        }
    }
    assert_eq!(samples.len(), 16);
}

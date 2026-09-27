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

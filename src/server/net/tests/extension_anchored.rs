//! Real nonblocking reactor, production replica assembly, custom own-state bytes.
use super::extension_lifecycle::{send, until};
use super::*;
use crate::{
    client::InventoryProbe,
    inventory::{Inventory, Stack},
    server::startup::ServerStartup,
};
use std::{sync::Arc, time::Instant};

#[test]
fn external_anchored_initialization_use_refund_and_restart_over_real_listener() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-anchored-tcp-{}-{stamp}",
        std::process::id()
    ));
    let mut identity = None;
    for restart in [false, true] {
        let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&bloxgloom_lifecycle_fixture::anchored::SignalPost)
            .unwrap();
        let mut state = Box::new(
            crate::server::server_state_with_startup(7, save.clone(), 8, startup).unwrap(),
        );
        let catalog = state.world.catalog_arc();
        let block = catalog
            .state_by_key(bloxgloom_lifecycle_fixture::anchored::KEY)
            .unwrap();
        let item = catalog
            .items()
            .find(|i| i.key == bloxgloom_lifecycle_fixture::anchored::KEY)
            .unwrap()
            .id;
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if !restart {
            for x in -2..=2 {
                for z in -2..=3 {
                    for y in 79..=82 {
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
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::new(item, 6));
            state.inventory_store.save(0xFACE, &inventory).unwrap();
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
                    name: "anchored-fixture".into(),
                    profile: 0xFACE,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut peer);
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let mut client = InventoryProbe::new(catalog.clone(), save.join("unused-config"));
            let mut session = false;
            let deadline = Instant::now() + Duration::from_secs(10);
            while !session || !client.ready(crate::world::world_to_chunk(0, 80, 2).0) {
                assert!(Instant::now() < deadline);
                let m = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
                session |= matches!(m, ServerMessage::ActionSession { .. });
                client.accept(m);
            }
            if !restart {
                let action_id = client.next_id();
                let place = ClientMessage::Edit {
                    action_id,
                    x: 0,
                    y: 80,
                    z: 2,
                    block,
                    slot: 0,
                };
                send(&mut peer, &mut client, place.clone(), &catalog);
                send(&mut peer, &mut client, place, &catalog);
            }
            until(&mut peer, &mut client, &catalog, |c| {
                c.anchored([0, 80, 2]).is_some()
            });
            let entity = client.anchored([0, 80, 2]).unwrap();
            assert_eq!(client.player_count(0), 3);
            if !restart {
                identity = Some(entity.id);
                assert_eq!(entity.payload, vec![0, 0]);
                // The real client's ray target is the non-anchor footprint cell.
                // No fixture request bytes or identifiers enter client dispatch.
                let request = client.action_on_block([0, 81, 2]);
                let ClientMessage::EntityInteract { ref payload, .. } = request else {
                    panic!()
                };
                let registered = bloxgloom_host_api::actions::Request::decode(payload).unwrap();
                assert_eq!(registered.entity, entity.id);
                assert_eq!(registered.entity_revision, entity.revision);
                send(&mut peer, &mut client, request.clone(), &catalog);
                send(&mut peer, &mut client, request, &catalog);
                until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored([0, 80, 2]).is_some_and(|e| e.payload == [1, 0])
                });
            } else {
                assert_eq!(Some(entity.id), identity);
                assert_eq!(entity.payload, vec![1, 0]);
                let action_id = client.next_id();
                let remove = ClientMessage::Edit {
                    action_id,
                    x: 0,
                    y: 81,
                    z: 2,
                    block: crate::world::AIR,
                    slot: 0,
                };
                send(&mut peer, &mut client, remove.clone(), &catalog);
                send(&mut peer, &mut client, remove, &catalog);
                until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored([0, 80, 2]).is_none()
                });
            }
            let _ = peer.shutdown(Shutdown::Both);
        }));
        stop_tx.send(()).unwrap();
        server.join().unwrap().unwrap();
        if let Err(p) = result {
            let _ = std::fs::remove_dir_all(&save);
            std::panic::resume_unwind(p)
        }
    }
    std::fs::remove_dir_all(save).unwrap();
}

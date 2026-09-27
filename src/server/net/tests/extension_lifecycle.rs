//! External declarations, real listener, production replica/UI/request paths.
use super::*;
use crate::client::InventoryProbe;
use crate::inventory::{Inventory, Stack};
use crate::server::startup::ServerStartup;
use std::sync::Arc;
use std::time::Instant;

fn send(
    peer: &mut TcpStream,
    client: &mut InventoryProbe,
    message: ClientMessage,
    catalog: &crate::content::Catalog,
) {
    let id = match &message {
        ClientMessage::Edit { action_id, .. } | ClientMessage::EntityInteract { action_id, .. } => {
            *action_id
        }
        _ => panic!("not an action"),
    };
    protocol::write_client_with_catalog(&mut *peer, &message, catalog).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline);
        let message = protocol::read_server_with_catalog(&mut *peer, catalog).unwrap();
        client.accept(message.clone());
        if let ServerMessage::ActionResult {
            action_id,
            accepted,
            reason,
        } = message
            && action_id == id
        {
            assert!(accepted, "{reason}");
            break;
        }
    }
}
fn until(
    peer: &mut TcpStream,
    client: &mut InventoryProbe,
    catalog: &crate::content::Catalog,
    ready: impl Fn(&InventoryProbe) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready(client) {
        assert!(Instant::now() < deadline);
        client.accept(protocol::read_server_with_catalog(&mut *peer, catalog).unwrap());
    }
}

#[test]
fn external_storage_screen_transfers_reopens_after_restart_and_breaks_over_real_listener() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-extension-ui-tcp-{}-{stamp}",
        std::process::id()
    ));
    for restarted in [false, true] {
        let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&bloxgloom_lifecycle_fixture::TallStore)
            .unwrap();
        let mut state = Box::new(
            crate::server::server_state_with_startup(7, save.clone(), 8, startup).unwrap(),
        );
        let catalog = state.world.catalog_arc();
        let block = catalog
            .state_by_key(bloxgloom_lifecycle_fixture::KEY)
            .unwrap();
        let item = catalog
            .items()
            .find(|i| i.key == bloxgloom_lifecycle_fixture::KEY)
            .unwrap()
            .id;
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if !restarted {
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
            inventory.slots[0] = Some(Stack::new(item, 2));
            inventory.slots[1] =
                Some(Stack::with_components(crate::items::STICK, 5, 1, vec![3, 9]).unwrap());
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
                    name: "extension-fixture".into(),
                    profile: 0xFACE,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, manifest) = receive_content_manifest(&mut peer);
            assert_eq!(fingerprint, catalog.fingerprint());
            assert_eq!(
                manifest,
                crate::content::ContentManifest::from_catalog(&catalog)
                    .encode()
                    .unwrap()
            );
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let mut client =
                InventoryProbe::new(catalog.clone(), save.join("unused-client-config"));
            let mut epoch = false;
            let key = crate::world::world_to_chunk(0, 80, 2).0;
            let deadline = Instant::now() + Duration::from_secs(10);
            while !epoch || !client.ready(key) {
                assert!(Instant::now() < deadline);
                let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
                epoch |= matches!(message, ServerMessage::ActionSession { .. });
                client.accept(message);
            }
            if !restarted {
                let id = client.next_id();
                let place = ClientMessage::Edit {
                    action_id: id,
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
            // Both footprint cells discover the registered screen; no fixture
            // ID appears anywhere in client input, protocol, layout, or drawing.
            client.open([0, 80, 2], block);
            client.close();
            client.open([0, 81, 2], block);
            if !restarted {
                let request = client.transfer(true, 1, 8, false);
                send(&mut peer, &mut client, request.clone(), &catalog);
                send(&mut peer, &mut client, request, &catalog);
                until(&mut peer, &mut client, &catalog, |c| {
                    c.view().unwrap().slots[8]
                        .as_ref()
                        .is_some_and(|s| s.count == 5)
                        && c.player_count(1) == 0
                });
                client.close();
                client.open([0, 80, 2], block);
                assert_eq!(client.view().unwrap().slots[8].as_ref().unwrap().count, 5);
            } else {
                assert_eq!(client.view().unwrap().slots[8].as_ref().unwrap().count, 5);
                let request = client.transfer(false, 1, 8, true);
                send(&mut peer, &mut client, request.clone(), &catalog);
                send(&mut peer, &mut client, request, &catalog);
                until(&mut peer, &mut client, &catalog, |c| {
                    c.view().unwrap().slots[8]
                        .as_ref()
                        .is_some_and(|s| s.count == 4)
                        && c.player_count(1) == 1
                });
                let request = client.transfer(false, 1, 8, false);
                send(&mut peer, &mut client, request, &catalog);
                until(&mut peer, &mut client, &catalog, |c| {
                    c.view().unwrap().slots[8].is_none() && c.player_count(1) == 5
                });
                client.close();
                let id = client.next_id();
                let remove = ClientMessage::Edit {
                    action_id: id,
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
        if let Err(panic) = result {
            let _ = std::fs::remove_dir_all(&save);
            std::panic::resume_unwind(panic);
        }
    }
    std::fs::remove_dir_all(save).unwrap();
}

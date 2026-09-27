//! External declarations on the real nonblocking reactor and replica assembler.
use super::*;
use crate::client::ReplicationProbe;
use crate::inventory::{Inventory, Stack};
use crate::server::startup::ServerStartup;
use std::sync::Arc;
use std::time::Instant;

fn send(
    peer: &mut TcpStream,
    replica: &mut ReplicationProbe,
    message: ClientMessage,
    id: u128,
    catalog: &crate::content::Catalog,
) {
    protocol::write_client_with_catalog(&mut *peer, &message, catalog).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline);
        let message = protocol::read_server_with_catalog(&mut *peer, catalog).unwrap();
        replica.accept(message.clone());
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

#[test]
fn external_storage_places_and_breaks_over_real_listener() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-extension-tcp-{}-{stamp}",
        std::process::id()
    ));
    let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&bloxgloom_lifecycle_fixture::TallStore)
        .unwrap();
    let mut state =
        Box::new(crate::server::server_state_with_startup(7, save.clone(), 8, startup).unwrap());
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
        protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint }).unwrap();
        let mut replica = ReplicationProbe::with_catalog(catalog.clone());
        let mut epoch = None;
        let key = crate::world::world_to_chunk(0, 80, 2).0;
        let deadline = Instant::now() + Duration::from_secs(10);
        while epoch.is_none() || !replica.contains_key(&key) {
            assert!(Instant::now() < deadline);
            let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
            if let ServerMessage::ActionSession { epoch: value, .. } = message {
                epoch = Some(value);
            }
            replica.accept(message);
        }
        let prefix = u128::from(epoch.unwrap()) << 64;
        let place = ClientMessage::Edit {
            action_id: prefix | 1,
            x: 0,
            y: 80,
            z: 2,
            block,
            slot: 0,
        };
        send(&mut peer, &mut replica, place.clone(), prefix | 1, &catalog);
        send(&mut peer, &mut replica, place, prefix | 1, &catalog);
        let deadline = Instant::now() + Duration::from_secs(10);
        while replica.anchored([0, 80, 2]).is_none() {
            assert!(Instant::now() < deadline);
            replica.accept(protocol::read_server_with_catalog(&mut peer, &catalog).unwrap());
        }
        let entity = replica.anchored([0, 80, 2]).unwrap();
        assert_eq!(
            entity.entity_type,
            catalog
                .entity_type_id_by_key(bloxgloom_lifecycle_fixture::KEY)
                .unwrap()
        );
        let mut payload = vec![2, 0, 8, 1];
        payload.extend(5u16.to_le_bytes());
        payload.extend(entity.id.to_le_bytes());
        payload.extend(entity.revision.to_le_bytes());
        send(
            &mut peer,
            &mut replica,
            ClientMessage::EntityInteract {
                action_id: prefix | 2,
                target: [0, 81, 2],
                payload,
            },
            prefix | 2,
            &catalog,
        );
        let remove = ClientMessage::Edit {
            action_id: prefix | 3,
            x: 0,
            y: 81,
            z: 2,
            block: crate::world::AIR,
            slot: 0,
        };
        send(
            &mut peer,
            &mut replica,
            remove.clone(),
            prefix | 3,
            &catalog,
        );
        send(&mut peer, &mut replica, remove, prefix | 3, &catalog);
        let deadline = Instant::now() + Duration::from_secs(10);
        while replica.anchored([0, 80, 2]).is_some() {
            assert!(Instant::now() < deadline);
            replica.accept(protocol::read_server_with_catalog(&mut peer, &catalog).unwrap());
        }
        let _ = peer.shutdown(Shutdown::Both);
    }));
    stop_tx.send(()).unwrap();
    server.join().unwrap().unwrap();
    std::fs::remove_dir_all(save).unwrap();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

//! Independent action package -> actual composed client control -> nonblocking
//! listener -> receipt/WAL -> inventory replica, including duplicate and restart.
use super::*;
use crate::{
    client::InventoryProbe,
    inventory::{Inventory, Stack},
    server::startup::ServerStartup,
};
use std::{sync::Arc, time::Instant};

fn send(
    peer: &mut TcpStream,
    client: &mut InventoryProbe,
    catalog: &crate::content::Catalog,
    request: &ClientMessage,
) -> bool {
    send_with_session(peer, client, catalog, request, true)
}
fn send_with_session(
    peer: &mut TcpStream,
    client: &mut InventoryProbe,
    catalog: &crate::content::Catalog,
    request: &ClientMessage,
    current_session: bool,
) -> bool {
    let ClientMessage::EntityInteract {
        action_id: expected,
        ..
    } = request
    else {
        panic!()
    };
    protocol::write_client_with_catalog(&mut *peer, request, catalog).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "registered action did not complete"
        );
        let message = protocol::read_server_with_catalog(&mut *peer, catalog).unwrap();
        if current_session
            || !matches!(message,ServerMessage::ActionResult { action_id, .. } if action_id == *expected)
        {
            client.accept(message.clone());
        }
        if let ServerMessage::ActionResult {
            action_id,
            accepted,
            ..
        } = message
            && action_id == *expected
        {
            return accepted;
        }
    }
}
#[test]
fn external_item_action_composed_control_receipt_duplicate_stale_and_restart() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-action-tcp-{}-{stamp}",
        std::process::id()
    ));
    let mut original = None;
    for restarted in [false, true] {
        let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&bloxgloom_lifecycle_fixture::Fixture)
            .unwrap();
        let mut state = Box::new(
            crate::server::server_state_with_startup(7, save.clone(), 8, startup).unwrap(),
        );
        let catalog = state.world.catalog_arc();
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if !restarted {
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::new(crate::items::ItemId(crate::world::GRAVEL.0), 6));
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
                    name: "action-proof".into(),
                    profile: 0xFACE,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut peer);
            assert_eq!(fingerprint, catalog.fingerprint());
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let mut client =
                InventoryProbe::new(catalog.clone(), save.join("unused-action-config"));
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut epoch = false;
            while !epoch || client.player_count(0) != if restarted { 4 } else { 6 } {
                assert!(Instant::now() < deadline);
                let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
                epoch |= matches!(message, ServerMessage::ActionSession { .. });
                client.accept(message);
            }
            if !restarted {
                let request = client.item_action(0);
                assert!(send(&mut peer, &mut client, &catalog, &request));
                while client.player_count(0) != 4 {
                    assert!(Instant::now() < deadline);
                    client.accept(protocol::read_server_with_catalog(&mut peer, &catalog).unwrap());
                }
                assert!(send(&mut peer, &mut client, &catalog, &request));
                assert_eq!(client.player_count(0), 4);
                assert_eq!(client.player_count(1), 3);
                let mut stale = request.clone();
                if let ClientMessage::EntityInteract { action_id, .. } = &mut stale {
                    *action_id = client.next_id();
                }
                assert!(!send(&mut peer, &mut client, &catalog, &stale));
                assert_eq!(client.player_count(0), 4);
                assert_eq!(client.player_count(1), 3);
                original = Some(request);
            } else {
                assert_eq!(client.player_count(0), 4);
                assert_eq!(client.player_count(1), 3);
                // The new connection epoch retires old-session actions. Replaying
                // one is rejected without reapplying its recovered inventory edit.
                // Its old-epoch reply is checked on the wire, not fed into the
                // fresh client's intentionally strict current-session tracker.
                assert!(!send_with_session(
                    &mut peer,
                    &mut client,
                    &catalog,
                    original.as_ref().unwrap(),
                    false
                ));
                assert_eq!(client.player_count(0), 4);
                assert_eq!(client.player_count(1), 3);
                let request = client.item_action(0);
                assert!(send(&mut peer, &mut client, &catalog, &request));
                while client.player_count(0) != 2 {
                    assert!(Instant::now() < deadline);
                    client.accept(protocol::read_server_with_catalog(&mut peer, &catalog).unwrap());
                }
                assert_eq!(client.player_count(0), 2);
                assert_eq!(client.player_count(1), 6);
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

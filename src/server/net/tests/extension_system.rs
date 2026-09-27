//! Public owner-system proof through the real nonblocking listener and recovery.
use super::*;
use crate::server::{parallel::OwnerKey, registry::SystemId, startup::ServerStartup};
use std::sync::Arc;

fn startup() -> ServerStartup {
    ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&bloxgloom_lifecycle_fixture::Fixture)
        .unwrap()
}
fn clock(state: &crate::server::State) -> (u64, u64) {
    let id = SystemId::new(bloxgloom_lifecycle_fixture::system::KEY).unwrap();
    let (revision, data) = state
        .system_runtime
        .owner_snapshot(
            &id,
            OwnerKey::Chunk(crate::world::ChunkKey { x: 0, y: 0, z: 0 }),
        )
        .unwrap();
    (
        revision,
        u64::from_le_bytes(
            data.get::<Vec<u8>>()
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap(),
        ),
    )
}

#[test]
fn external_system_runs_without_entities_and_recovers_across_real_listener_restart() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-system-tcp-{}-{stamp}",
        std::process::id()
    ));
    let mut previous = (0, 0);
    for round in 0..2 {
        let state = Box::new(
            crate::server::server_state_with_startup(7, save.clone(), 2, startup()).unwrap(),
        );
        let initial = clock(&state);
        if round == 0 {
            assert_eq!(initial.1, 0);
        } else {
            assert_eq!(initial, previous);
        }
        let catalog = state.world.catalog_arc();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stop_rx));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "system-proof".into(),
                    profile: 0xface,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut peer);
            assert_eq!(fingerprint, catalog.fingerprint());
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            loop {
                if matches!(
                    protocol::read_server_with_catalog(&mut peer, &catalog).unwrap(),
                    ServerMessage::ActionSession { .. }
                ) {
                    break;
                }
            }
            // Cross at least one persisted fifty-tick deadline even after restart.
            thread::sleep(Duration::from_millis(1200));
            protocol::write_client(&mut peer, &ClientMessage::Ping { nonce: 42 }).unwrap();
            loop {
                if matches!(
                    protocol::read_server_with_catalog(&mut peer, &catalog).unwrap(),
                    ServerMessage::Pong { nonce: 42 }
                ) {
                    break;
                }
            }
            let _ = peer.shutdown(Shutdown::Both);
        }));
        stop_tx.send(()).unwrap();
        server.join().unwrap().unwrap();
        if let Err(panic) = result {
            let _ = std::fs::remove_dir_all(&save);
            std::panic::resume_unwind(panic);
        }
        let recovered =
            crate::server::server_state_with_startup(7, save.clone(), 2, startup()).unwrap();
        previous = clock(&recovered);
        assert!(previous.0 > initial.0);
        assert!(previous.1 > initial.1);
        drop(recovered);
    }
    std::fs::remove_dir_all(save).unwrap();
}

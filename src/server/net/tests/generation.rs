//! Startup registration reaches real loader workers and the nonblocking listener.
use super::*;
use bloxgloom_host_api::generation::{Context, Contributor, GenerationError, Output, Registration};

struct Marker;
impl Contributor for Marker {
    fn generate(&self, _: Context, output: &mut Output) -> Result<(), GenerationError> {
        output.set([0, 0, 0], "bloxgloom:glowstone")
    }
}
impl bloxgloom_host_api::Extension for Marker {
    fn register(
        &self,
        host: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), bloxgloom_host_api::RegistrationError> {
        host.generation_contributor(Registration {
            key: "sample:marker".into(),
            revision: 1,
            contributor: Arc::new(Marker),
        })
    }
}

#[test]
fn registered_generation_streams_after_cache_miss_and_server_restart() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-generation-tcp-{}-{stamp}",
        std::process::id()
    ));
    for _ in 0..2 {
        let startup = crate::server::startup::ServerStartup::new(Arc::new(
            crate::content::Catalog::builtins(),
        ))
        .with_extension(&Marker)
        .unwrap();
        let mut state = Box::new(
            crate::server::server_state_with_startup(7, save.clone(), 8, startup).unwrap(),
        );
        // Force joins and streaming to go through ChunkLoader, not startup's cache.
        state
            .world
            .reset_cache_for_test(crate::server::SERVER_CHUNK_CACHE);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stop_rx));
        let result = std::panic::catch_unwind(|| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "generation-test".into(),
                    profile: 0x67656e,
                    content_fingerprint: crate::content::catalog().fingerprint(),
                },
            )
            .unwrap();
            complete_content_handshake(&mut peer);
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                assert!(
                    std::time::Instant::now() < deadline,
                    "generated chunk did not stream"
                );
                if let ServerMessage::WorldSnapshotStart(snapshot) =
                    protocol::read_server(&mut peer).unwrap()
                {
                    assert_eq!(
                        snapshot.chunk.block([0, 0, 0]),
                        Some(crate::world::GLOWSTONE)
                    );
                    break;
                }
            }
            let _ = peer.shutdown(Shutdown::Both);
        });
        stop_tx.send(()).unwrap();
        server.join().unwrap().unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
    std::fs::remove_dir_all(save).unwrap();
}

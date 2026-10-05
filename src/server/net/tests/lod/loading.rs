//! Opt-in throughput measurement over the production nonblocking listener.
use super::*;
use std::collections::HashMap;

fn connect(address: std::net::SocketAddr) -> (TcpStream, Vec<crate::lod::TileKey>) {
    let mut peer = TcpStream::connect(address).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "lod-loading".into(),
            profile: 0x10_d10,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
    )
    .unwrap();
    complete_content_handshake(&mut peer);
    let position = loop {
        if let ServerMessage::Position { x, y, z, .. } = protocol::read_server(&mut peer).unwrap() {
            break glam::Vec3::new(x, y, z);
        }
    };
    protocol::write_client(&mut peer, &ClientMessage::SetView { radius: 1 }).unwrap();
    protocol::write_client(&mut peer, &ClientMessage::LodConfig { horizon: 512 }).unwrap();
    loop {
        if let ServerMessage::LodStatus {
            horizon, max_level, ..
        } = protocol::read_server(&mut peer).unwrap()
        {
            assert_eq!(horizon, 512);
            return (
                peer,
                crate::render::lod::desired_tiles(position, horizon, 1, max_level),
            );
        }
    }
}

fn measure(peer: &mut TcpStream, keys: &[crate::lod::TileKey], first_id: u64, label: &str) {
    let started = Instant::now();
    let mut next = 0;
    let mut completed = 0;
    let mut pending = HashMap::new();
    let mut samples = Vec::new();
    let mut first = None;
    let mut pings = HashMap::new();
    let mut worst_ping = Duration::ZERO;
    while completed < keys.len() {
        while pending.len() < 4 && next < keys.len() {
            let request = first_id + next as u64;
            protocol::write_client(
                &mut *peer,
                &ClientMessage::LodRequest {
                    request,
                    key: keys[next],
                },
            )
            .unwrap();
            pending.insert(request, (keys[next], Instant::now()));
            next += 1;
        }
        if pings.is_empty() {
            let nonce = first_id + completed as u64;
            protocol::write_client(&mut *peer, &ClientMessage::Ping { nonce }).unwrap();
            pings.insert(nonce, Instant::now());
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "LOD throughput timeout"
        );
        match protocol::read_server(&mut *peer).unwrap() {
            ServerMessage::LodTile { request, tile, .. } if pending.contains_key(&request) => {
                let (key, sent) = pending.remove(&request).unwrap();
                assert_eq!(tile.key, key);
                tile.validate(crate::content::catalog()).unwrap();
                first.get_or_insert(started.elapsed());
                samples.push(sent.elapsed());
                completed += 1;
            }
            ServerMessage::LodUnavailable { request, key, .. }
                if pending.contains_key(&request) =>
            {
                panic!("tile unavailable: {key:?}")
            }
            ServerMessage::Pong { nonce } => {
                if let Some(sent) = pings.remove(&nonce) {
                    worst_ping = worst_ping.max(sent.elapsed());
                }
            }
            _ => {}
        }
    }
    samples.sort_unstable();
    eprintln!(
        "LOD loading {label}: tiles={} window=4 first={:.3}ms total={:.3}ms request_p50={:.3}ms request_p95={:.3}ms worst_ping={:.3}ms",
        keys.len(),
        first.unwrap().as_secs_f64() * 1000.,
        started.elapsed().as_secs_f64() * 1000.,
        samples[samples.len() / 2].as_secs_f64() * 1000.,
        samples[samples.len() * 95 / 100].as_secs_f64() * 1000.,
        worst_ping.as_secs_f64() * 1000.
    );
}

#[test]
#[ignore = "manual cold/memory/restart LOD throughput benchmark"]
fn lod_loading_throughput_over_real_listener() {
    let _logging = crate::logging::init().unwrap();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-lod-loading-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let (address, server) = crate::server::start_local_server(7, save.clone()).unwrap();
    let (mut peer, keys) = connect(address);
    measure(&mut peer, &keys, 100, "cold");
    measure(&mut peer, &keys, 1000, "repeat");
    drop(peer);
    server.stop().unwrap();
    let (address, server) = crate::server::start_local_server(7, save.clone()).unwrap();
    let (mut peer, keys) = connect(address);
    measure(&mut peer, &keys, 2000, "restart");
    drop(peer);
    server.stop().unwrap();
    std::fs::remove_dir_all(save).unwrap();
}

struct Gate {
    entered: std::sync::mpsc::SyncSender<()>,
    release: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}
impl bloxgloom_host_api::generation::Contributor for Gate {
    fn generate(
        &self,
        context: bloxgloom_host_api::generation::Context,
        _: &mut bloxgloom_host_api::generation::Output,
    ) -> Result<(), bloxgloom_host_api::generation::GenerationError> {
        if context.chunk == [8, 0, 8]
            && thread::current()
                .name()
                .is_some_and(|name| name.starts_with("lod-terrain"))
        {
            let _ = self.entered.try_send(());
            let (lock, condition) = &*self.release;
            let released = lock.lock().unwrap();
            let _ = condition
                .wait_timeout_while(released, Duration::from_secs(20), |released| !*released)
                .unwrap();
        }
        Ok(())
    }
}
struct GatedExtension(Arc<Gate>);
impl bloxgloom_host_api::Extension for GatedExtension {
    fn register(
        &self,
        host: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), bloxgloom_host_api::RegistrationError> {
        host.generation_contributor(bloxgloom_host_api::generation::Registration {
            key: "lod:loading_gate".into(),
            revision: 1,
            contributor: self.0.clone(),
        })
    }
}

#[test]
fn later_lod_request_and_ping_progress_while_first_generation_is_blocked() {
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-lod-gate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let (entered, started) = mpsc::sync_channel(1);
    let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&GatedExtension(Arc::new(Gate {
                entered,
                release: release.clone(),
            })))
            .unwrap();
    let mut state =
        Box::new(crate::server::server_state_with_startup(7, save.clone(), 128, startup).unwrap());
    state.lod = crate::server::lod::Service::with_workers(&state.world, save.clone(), 2).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = mpsc::sync_channel(1);
    let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stopped));
    let result = std::panic::catch_unwind(|| {
        let (mut peer, _) = connect(address);
        protocol::write_client(
            &mut peer,
            &ClientMessage::LodRequest {
                request: 1,
                key: crate::lod::TileKey {
                    level: 0,
                    x: 4,
                    z: 4,
                },
            },
        )
        .unwrap();
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::LodRequest {
                request: 2,
                key: crate::lod::TileKey {
                    level: 0,
                    x: 5,
                    z: 4,
                },
            },
        )
        .unwrap();
        protocol::write_client(&mut peer, &ClientMessage::Ping { nonce: 123 }).unwrap();
        let mut tile = false;
        let mut ping = false;
        while !(tile && ping) {
            match protocol::read_server(&mut peer).unwrap() {
                ServerMessage::LodTile { request: 2, .. } => tile = true,
                ServerMessage::LodTile { request: 1, .. } => {
                    panic!("first tile must still be blocked")
                }
                ServerMessage::LodUnavailable { request: 1 | 2, .. } => {
                    panic!("bounded tile should be available")
                }
                ServerMessage::Pong { nonce: 123 } => ping = true,
                _ => {}
            }
        }
        assert!(!*release.0.lock().unwrap());
    });
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    stop.send(()).unwrap();
    server.join().unwrap().unwrap();
    std::fs::remove_dir_all(save).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

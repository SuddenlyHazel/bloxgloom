//! A slow package peer and repeated fresh transfers alongside live gameplay.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) fn state_with_package(save: &std::path::Path) -> Box<crate::server::State> {
    let package = save.join("packages/loadtest");
    for directory in ["server", "client", "assets/textures"] {
        std::fs::create_dir_all(package.join(directory)).unwrap();
    }
    let mut manifest = String::from(
        "format 2\npackage loadtest\nversion 1.0.0\nentry main\nmodule server main server/main.luau\nmodule client view client/view.luau\n",
    );
    for index in 0..6 {
        manifest.push_str(&format!(
            "asset texture blob{index} assets/textures/blob{index}.png\n"
        ));
        std::fs::write(
            package.join(format!("assets/textures/blob{index}.png")),
            vec![0x5a; 256 * 1024],
        )
        .unwrap();
    }
    std::fs::write(package.join("package.txt"), manifest).unwrap();
    std::fs::write(package.join("server/main.luau"), "return function(_) end").unwrap();
    std::fs::write(
        package.join("client/view.luau"),
        "return function(_) return {} end",
    )
    .unwrap();
    // Unreferenced opaque asset exercises a large legal transfer without adding
    // content IDs or unrelated GPU work to this network/scheduling measurement.
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_local_packages(&std::fs::canonicalize(save.join("packages")).unwrap())
            .unwrap();
    Box::new(crate::server::server_state_with_startup(7, save.join("world"), 8, startup).unwrap())
}

pub(super) fn receive_package(peer: &mut TcpStream) {
    let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut *peer).unwrap() else {
        panic!("missing bundle offer")
    };
    let bundle = crate::client::bundle::receive(peer, identity, None).unwrap();
    crate::client::startup::prepare(bundle.clone()).unwrap();
    complete_package_handshake(peer, &bundle);
}

fn complete_package_handshake(
    peer: &mut TcpStream,
    bundle: &crate::server::client_bundle::ClientBundle,
) {
    let (fingerprint, bytes) = receive_content_manifest(peer);
    let manifest = crate::content::ContentManifest::decode(&bytes).unwrap();
    let catalog = manifest
        .resolve_catalog(&bundle.session_catalog().unwrap())
        .unwrap();
    assert_eq!(fingerprint, catalog.fingerprint());
    protocol::write_client(peer, &ClientMessage::ContentReady { fingerprint }).unwrap();
}

pub(super) struct Downloads {
    stop: Arc<AtomicBool>,
    release: Option<mpsc::SyncSender<()>>,
    worker: Option<thread::JoinHandle<(usize, usize)>>,
}
impl Downloads {
    pub(super) fn start(address: std::net::SocketAddr) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let (release, released) = mpsc::sync_channel(1);
        let (ready, started) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let mut completed = 0;
            let mut cancelled = 0;
            for round in 0..100 {
                if stopped.load(Ordering::Acquire) {
                    break;
                }
                let mut peer = TcpStream::connect(address).unwrap();
                peer.set_nodelay(true).unwrap();
                peer.set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                protocol::write_client(
                    &mut peer,
                    &ClientMessage::Hello {
                        name: "package-load".into(),
                        profile: 0xd000 + round,
                        content_fingerprint: crate::content::catalog().fingerprint(),
                    },
                )
                .unwrap();
                let ServerMessage::BundleOffer { identity } =
                    protocol::read_server(&mut peer).unwrap()
                else {
                    panic!("missing offer")
                };
                let result = crate::client::bundle::receive_progress(
                    &mut peer,
                    identity,
                    None,
                    |bytes, _| {
                        if round == 0 && bytes == protocol::MAX_BUNDLE_PART as u32 {
                            ready.send(()).unwrap();
                            released.recv_timeout(Duration::from_secs(10)).unwrap();
                        }
                        if stopped.load(Ordering::Acquire) || (round % 3 == 2 && bytes > 0) {
                            return Err(io::Error::new(
                                io::ErrorKind::Interrupted,
                                "cancel package transfer",
                            ));
                        }
                        // Simulate a bounded-bandwidth receiver, independently of
                        // the real nonblocking server listener and live peer.
                        if bytes > 0 {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Ok(())
                    },
                );
                if result.is_err() && round % 3 == 2 {
                    cancelled += 1;
                }
                if let Ok(bundle) = result {
                    crate::client::startup::prepare(bundle.clone()).unwrap();
                    complete_package_handshake(&mut peer, &bundle);
                    assert!(matches!(
                        protocol::read_server(&mut peer).unwrap(),
                        ServerMessage::Welcome { .. }
                    ));
                    completed += 1;
                }
                let _ = peer.shutdown(Shutdown::Both);
            }
            (completed, cancelled)
        });
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        Self {
            stop,
            release: Some(release),
            worker: Some(worker),
        }
    }
    pub(super) fn resume(&mut self) {
        self.release.take().unwrap().send(()).unwrap();
    }
    pub(super) fn finish(mut self) {
        self.stop.store(true, Ordering::Release);
        let (completed, cancelled) = self.worker.take().unwrap().join().unwrap();
        assert!(
            completed >= 2,
            "insufficient overlapping transfers: {completed}"
        );
        assert!(cancelled >= 1, "no cancelled transfers overlapped gameplay");
        eprintln!(
            "mixed package load: {completed} complete 1.5 MiB transfers and {cancelled} cancelled attempts"
        );
    }
}
impl Drop for Downloads {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

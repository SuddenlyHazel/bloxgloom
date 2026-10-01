//! Explicit generated maximum-count fixture, separate from normal gameplay tests.
use super::*;
use std::time::Duration;

#[test]
#[ignore = "generate-pressure.py output required in BLOXGLOOM_PRESSURE_PACKAGES"]
fn farming_pressure_maximum_counts_cross_old_bytes_and_join_over_real_tcp() {
    let fixture = Fixture::new();
    let packages = std::path::PathBuf::from(
        std::env::var_os("BLOXGLOOM_PRESSURE_PACKAGES")
            .expect("set BLOXGLOOM_PRESSURE_PACKAGES to generate-pressure.py output"),
    );
    let started = Instant::now();
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&packages)
        .unwrap();
    eprintln!("pressure discovery/startup: {:?}", started.elapsed());
    let state = Box::new(
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap(),
    );
    gameplay::serve(state, |address| {
        let started = Instant::now();
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_nodelay(true).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "maximum-pressure".into(),
                profile: PROFILE + 999,
                content_fingerprint: Catalog::builtins().fingerprint(),
            },
        )
        .unwrap();
        let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut peer).unwrap()
        else {
            panic!("missing pressure offer")
        };
        let bundle = crate::client::bundle::receive(&mut peer, identity, None).unwrap();
        assert_eq!(bundle.packages().len(), 4);
        assert_eq!(
            bundle
                .packages()
                .values()
                .map(|package| package.sources.len())
                .sum::<usize>(),
            1020
        );
        assert_eq!(
            bundle
                .packages()
                .values()
                .map(|package| package.textures.len())
                .sum::<usize>(),
            1024
        );
        assert!(bundle.bytes().len() > 4 * 1024 * 1024);
        for package in bundle.packages().values() {
            assert_eq!(package.sources.len(), 255); // the 256th server entry is excluded
            assert_eq!(package.textures.len(), 256);
        }
        let prepare = Instant::now();
        crate::client::startup::prepare(bundle.clone()).unwrap();
        eprintln!(
            "pressure client source preparation: {:?}",
            prepare.elapsed()
        );
        let (fingerprint, bytes) = receive_content_manifest(&mut peer);
        let manifest = crate::content::ContentManifest::decode(&bytes).unwrap();
        let catalog = manifest
            .resolve_catalog(&bundle.session_catalog().unwrap())
            .unwrap();
        assert_eq!(fingerprint, catalog.fingerprint());
        protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint }).unwrap();
        loop {
            if matches!(
                protocol::read_server(&mut peer).unwrap(),
                ServerMessage::Welcome { .. }
            ) {
                break;
            }
        }
        let encoded = bundle.bytes().len();
        eprintln!(
            "pressure realTCP cold join={:?},encoded={encoded} bytes,verification reservation={} bytes,decoded RGBA source pixels={} bytes",
            started.elapsed(),
            encoded * 3 + crate::protocol::MAX_FRAME * 2,
            1024 * 64 * 64 * 4
        );
        // These assets are intentionally unregistered: no GPU texture array is
        // allocated, and source pixel count is not a measured GPU allocation.
    });
}

//! Cold transfer versus verified-cache reconnect preparation, over real TCP.
use super::*;
use std::time::Duration;

#[test]
fn farming_scale_cold_and_cached_join_latency() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let state = Box::new(open(&fixture));
    gameplay::serve(state, |address| {
        let mut cached = None;
        let mut cold = Vec::new();
        let mut warm = Vec::new();
        for sample in 0..12u128 {
            for reuse in [false, true] {
                let started = Instant::now();
                let mut peer = TcpStream::connect(address).unwrap();
                peer.set_nodelay(true).unwrap();
                peer.set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                protocol::write_client(
                    &mut peer,
                    &ClientMessage::Hello {
                        name: "farming-join-measurement".into(),
                        profile: PROFILE + 100 + sample * 2 + u128::from(reuse),
                        content_fingerprint: Catalog::builtins().fingerprint(),
                    },
                )
                .unwrap();
                let ServerMessage::BundleOffer { identity } =
                    protocol::read_server(&mut peer).unwrap()
                else {
                    panic!("missing package offer")
                };
                let bundle = crate::client::bundle::receive(
                    &mut peer,
                    identity,
                    if reuse { cached.clone() } else { None },
                )
                .unwrap();
                crate::client::startup::prepare(bundle.clone()).unwrap();
                let (fingerprint, bytes) = receive_content_manifest(&mut peer);
                let manifest = crate::content::ContentManifest::decode(&bytes).unwrap();
                let catalog = manifest
                    .resolve_catalog(&bundle.session_catalog().unwrap())
                    .unwrap();
                assert_eq!(fingerprint, catalog.fingerprint());
                protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                    .unwrap();
                loop {
                    if matches!(
                        protocol::read_server(&mut peer).unwrap(),
                        ServerMessage::Welcome { .. }
                    ) {
                        break;
                    }
                }
                let elapsed = started.elapsed();
                if reuse {
                    warm.push(elapsed);
                } else {
                    cold.push(elapsed);
                }
                cached = Some(bundle);
                drop(peer);
                thread::sleep(Duration::from_millis(20));
            }
        }
        let bundle = cached.unwrap();
        let encoded = bundle.bytes().len();
        eprintln!(
            "farming encoded bundle={encoded} bytes; transfer/verification reservation={} bytes (three payloads plus two frame scratch buffers)",
            encoded * 3 + crate::protocol::MAX_FRAME * 2
        );
        report("cold verified join/preparation", &mut cold);
        report("cached verified reconnect/preparation", &mut warm);
    });
}

fn report(label: &str, samples: &mut [Duration]) {
    samples.sort_unstable();
    let index = |percent| (samples.len() * percent).div_ceil(100).saturating_sub(1);
    eprintln!(
        "farming {label}: samples={},p50={:?},p95={:?},p99={:?},max={:?}",
        samples.len(),
        samples[index(50)],
        samples[index(95)],
        samples[index(99)],
        samples.last().unwrap()
    );
    assert!(
        *samples.last().unwrap() < Duration::from_secs(2),
        "join failed to make bounded progress"
    );
}

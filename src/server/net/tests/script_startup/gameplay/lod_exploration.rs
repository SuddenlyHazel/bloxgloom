//! Sustained authoritative exploration outgrows both derived cache capacities.
use super::*;

#[test]
#[ignore = "130 cold authoritative teleports; run manually for sustained exploration pressure"]
fn lod_exploration_revisits_evicted_tiles_without_pinning_authority() {
    let fixture = Fixture::new();
    fixture.action(
        "return function(h) h.register_action('demo:shift',1,'Explore','empty',nil,'demo:action') end",
        r#"return function(c,e)
            local me=c.player_by_profile(c.player_profile)
            local index=string.byte(e.arguments,1)
            c.teleport_player(me.session,index*512+8.5,300,8.5)
        end"#,
    );
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(manifest, format!("{text}requires bloxgloom:players/v1\n")).unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let (sample_tx, sample_rx) = mpsc::sync_channel(8192);
    state.tick_observer = Some(sample_tx);
    let started = Instant::now();
    let mut worst_ping = Duration::ZERO;
    let mut worst_movement = Duration::ZERO;
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        peer.write(&ClientMessage::SetView { radius: 1 });
        peer.write(&ClientMessage::LodConfig { horizon: 512 });
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if let ServerMessage::LodStatus {
                horizon, max_level, ..
            } = peer.read(deadline)
            {
                assert_eq!((horizon, max_level), (512, 4));
                break;
            }
        }
        // 130 different roots exceed the 64-entry memory cache and the
        // 128-file disk cache. The last return must recover an evicted root.
        for (visit, index) in (0u8..130).chain(std::iter::once(0)).enumerate() {
            let request = peer.request(index);
            let ClientMessage::EntityInteract { action_id, .. } = request else {
                unreachable!()
            };
            peer.write(&request);
            let mut reset = None;
            let mut accepted = false;
            while reset.is_none() || !accepted {
                match peer.read(deadline) {
                    ServerMessage::PlayerTeleport {
                        session,
                        reset: r,
                        position,
                        ..
                    } => {
                        assert_eq!(session, peer.epoch);
                        assert_eq!(position, [f32::from(index) * 512.0 + 8.5, 300.0, 8.5]);
                        reset = Some(r);
                    }
                    ServerMessage::ActionResult {
                        action_id: id,
                        accepted: ok,
                        reason,
                    } if id == action_id => {
                        assert!(ok, "visit {visit}: {reason}");
                        accepted = true;
                    }
                    _ => {}
                }
            }
            peer.write(&ClientMessage::ActionAck {
                epoch: peer.epoch,
                through_seq: action_id as u64,
            });
            let seq = visit as u64 + 1;
            peer.write(&ClientMessage::MovementReady {
                session: peer.epoch,
                reset: reset.unwrap(),
                next_seq: seq,
            });
            let movement_started = Instant::now();
            peer.write(&ClientMessage::Move {
                seq,
                dx: 0.1,
                dy: 0.0,
                dz: 0.0,
            });
            let key = crate::lod::TileKey::containing(4, i32::from(index) * 512, 0).unwrap();
            let id = visit as u64 + 1;
            peer.write(&ClientMessage::LodRequest { request: id, key });
            let ping_started = Instant::now();
            peer.write(&ClientMessage::Ping { nonce: id });
            let (mut tile, mut pong, mut movement) = (false, false, false);
            while !tile || !pong || !movement {
                match peer.read(deadline) {
                    ServerMessage::LodTile {
                        session,
                        request,
                        tile: summary,
                    } if request == id => {
                        assert_eq!(session, peer.epoch);
                        assert_eq!(summary.key, key);
                        summary.validate(&peer.catalog).unwrap();
                        tile = true;
                    }
                    ServerMessage::LodUnavailable { request, .. } if request == id => {
                        panic!("visit {visit} unavailable")
                    }
                    ServerMessage::Pong { nonce } if nonce == id => {
                        worst_ping = worst_ping.max(ping_started.elapsed());
                        pong = true;
                    }
                    ServerMessage::Position { ack_seq, x, y, z } if ack_seq == seq => {
                        assert!((x - (f32::from(index) * 512.0 + 8.6)).abs() < 0.02);
                        assert_eq!((y, z), (300.0, 8.5));
                        worst_movement = worst_movement.max(movement_started.elapsed());
                        movement = true;
                    }
                    _ => {}
                }
            }
        }
        peer.stream.shutdown(std::net::Shutdown::Both).unwrap();
        // Observe actual disconnect retirement while the nonblocking reactor
        // still runs, rather than inferring pin release from State destruction.
        let mut samples: Vec<_> = sample_rx.try_iter().collect();
        loop {
            let sample = sample_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            samples.push(sample);
            if sample.active_clients == 0 && sample.pinned_chunks == 0 {
                assert!(
                    sample.resident_chunks < 20_000,
                    "LOD populated authoritative chunks"
                );
                break;
            }
        }
        for sample in &samples {
            assert!(sample.replication_queue_depth <= 128);
            assert!(sample.replication_bytes_queued <= 2 * 1024 * 1024);
            assert_eq!(sample.active_drops, 0);
            assert!(
                sample.pinned_chunks < 512,
                "exploration retained stale subscriptions"
            );
        }
        eprintln!(
            "LOD exploration authority: resident_max={} pinned_max={} replication_bytes_max={}",
            samples.iter().map(|s| s.resident_chunks).max().unwrap(),
            samples.iter().map(|s| s.pinned_chunks).max().unwrap(),
            samples
                .iter()
                .map(|s| s.replication_bytes_queued)
                .max()
                .unwrap()
        );
    });
    let files: Vec<_> = std::fs::read_dir(fixture.0.join("save/lod-cache"))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        files.len(),
        128,
        "derived cache did not reach/retain its hard cap"
    );
    let bytes: u64 = files
        .iter()
        .map(|file| file.metadata().unwrap().len())
        .sum();
    assert!(bytes <= 128 * (crate::lod::MAX_TILE_BYTES as u64 + 64));
    assert!(worst_ping < Duration::from_secs(2));
    assert!(worst_movement < Duration::from_secs(2));
    eprintln!(
        "LOD exploration: 130 roots + revisit, elapsed={}ms ping_max={}ms movement_max={}ms disk_files={} disk_bytes={bytes}",
        started.elapsed().as_millis(),
        worst_ping.as_millis(),
        worst_movement.as_millis(),
        files.len()
    );
}

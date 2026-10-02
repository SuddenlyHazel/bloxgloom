use super::*;
use std::{fs, path::PathBuf};
fn temporary() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("bloxgloom-weather-{}-{nonce}", std::process::id()));
    fs::create_dir(&root).unwrap();
    root
}
#[test]
fn weather_restart_preserves_transition_and_wal_recovers_unapplied_override() {
    use crate::server::journal::{Journal, Transaction};
    let root = temporary();
    let mut clock = Clock::open(&root, 7).unwrap();
    let first = clock.prepare(1, 30_000).unwrap();
    let writer = Journal::open(root.join("server.wal"))
        .unwrap()
        .into_writer(4, Duration::from_millis(1))
        .unwrap();
    writer
        .try_submit(Transaction::new(1, 1, vec![first.clone()]))
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    clock.apply(first).unwrap();
    clock.finish().unwrap();
    let mut resumed = Clock::open(&root, 99).unwrap();
    assert_eq!(resumed.snapshot().seed, 7);
    assert_eq!(resumed.snapshot().to, WeatherKind::Rain);
    assert_eq!(resumed.snapshot().transition_duration_ms, 30_000);
    let second = resumed.prepare(2, 0).unwrap();
    writer
        .try_submit(Transaction::new(2, 2, vec![second]))
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    resumed.finish().unwrap();
    writer
        .try_rotate(writer.sequence())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    drop(writer);
    let journal = Journal::open(root.join("server.wal")).unwrap();
    let recovered = prepare_recovery(&root, &journal, &journal.latest_values())
        .unwrap()
        .unwrap();
    replay(&root, &recovered).unwrap();
    let mut recovered = Clock::open(&root, 7).unwrap();
    assert_eq!(recovered.snapshot().to, WeatherKind::Storm);
    assert_eq!(
        recovered
            .snapshot()
            .sample_at(recovered.snapshot().elapsed_ms)
            .rain,
        1.
    );
    recovered.finish().unwrap();
    drop(clock);
    drop(resumed);
    drop(recovered);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn real_listener_synchronizes_admin_weather_and_denies_other_players() {
    use crate::protocol::{self, ClientMessage, ServerMessage};
    use bloxgloom_host_api::actions::Request;
    use std::net::TcpStream;
    let root = temporary();
    let connect = |address, profile| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "weather-test".into(),
                profile,
                content_fingerprint: crate::content::catalog().fingerprint(),
            },
        )
        .unwrap();
        let mut epoch = 0;
        let mut inventory = 0;
        loop {
            match protocol::read_server(&mut peer).unwrap() {
                ServerMessage::ContentManifestPart {
                    offset,
                    total_len,
                    bytes,
                    ..
                } if offset as usize + bytes.len() == total_len as usize => {
                    protocol::write_client(
                        &mut peer,
                        &ClientMessage::ContentReady {
                            fingerprint: crate::content::catalog().fingerprint(),
                        },
                    )
                    .unwrap();
                }
                ServerMessage::ActionSession { epoch: value, .. } => epoch = value,
                ServerMessage::Inventory { revision, .. } => inventory = revision,
                ServerMessage::Weather { snapshot } => break (peer, epoch, inventory, snapshot),
                _ => {}
            }
        }
    };
    let send = |peer: &mut TcpStream, epoch: u64, revision: u64, kind: u8| {
        let mut arguments = vec![kind];
        arguments.extend(0u32.to_le_bytes());
        let payload = Request {
            key: crate::gameplay::admin::WEATHER.into(),
            version: 1,
            slot: 0,
            inventory_revision: revision,
            entity: 0,
            entity_revision: 0,
            arguments,
        }
        .encode()
        .unwrap();
        protocol::write_client(
            peer,
            &ClientMessage::EntityInteract {
                action_id: (u128::from(epoch) << 64) | 1,
                target: [0, 1, 0],
                payload,
            },
        )
        .unwrap();
    };
    let (address, server) =
        crate::server::start_local_server_with_admin(7, root.clone(), 0x8675).unwrap();
    let (mut peer, epoch, inventory, first) = connect(address, 0x8675);
    let (mut other, other_epoch, other_inventory, second) = connect(address, 0x8676);
    assert_eq!(first.seed, second.seed);
    assert_eq!(first.to, WeatherKind::Clear);
    send(
        &mut other,
        other_epoch,
        other_inventory,
        WeatherKind::StormSevere as u8,
    );
    loop {
        if let ServerMessage::ActionResult {
            accepted: false, ..
        } = protocol::read_server(&mut other).unwrap()
        {
            break;
        }
    }
    send(&mut peer, epoch, inventory, WeatherKind::StormSevere as u8);
    let changed = loop {
        if let ServerMessage::Weather { snapshot } = protocol::read_server(&mut peer).unwrap()
            && snapshot.to == WeatherKind::StormSevere
        {
            break snapshot;
        }
    };
    loop {
        if let ServerMessage::Weather { snapshot } = protocol::read_server(&mut other).unwrap()
            && snapshot.to == WeatherKind::StormSevere
        {
            assert_eq!(snapshot.revision, changed.revision);
            break;
        }
    }
    server.stop().unwrap();
    drop(peer);
    drop(other);
    let (address, server) = crate::server::start_local_server(7, root.clone()).unwrap();
    let (peer, _, _, resumed) = connect(address, 0x8675);
    assert_eq!(resumed.to, WeatherKind::StormSevere);
    assert_eq!(resumed.revision, changed.revision);
    server.stop().unwrap();
    drop(peer);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn natural_weather_is_independent_of_poll_partitioning() {
    let initial = WeatherSnapshot::initial(123);
    let expected = advance(initial, 1_000_000);
    let mut sampled = initial;
    for elapsed in (0..=1_000_000).step_by(500) {
        sampled = advance(sampled, elapsed);
    }
    assert_eq!(sampled, expected);
    assert!(expected.valid());
}

#[test]
fn durable_apply_does_not_rewind_elapsed_weather_time() {
    let root = temporary();
    let mut clock = Clock::open(&root, 7).unwrap();
    let change = clock.prepare(2, 30_000).unwrap();
    clock.started -= Duration::from_secs(2);
    let before = clock.snapshot().elapsed_ms;
    clock.apply(change).unwrap();
    assert!(clock.snapshot().elapsed_ms >= before);
    clock.finish().unwrap();
    drop(clock);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn severity_transitions_remain_continuous_and_survive_checkpoint() {
    let root = temporary();
    let mut clock = Clock::open(&root, 42).unwrap();
    clock
        .apply(clock.prepare(WeatherKind::StormSevere as u8, 0).unwrap())
        .unwrap();
    let severe = clock.snapshot();
    assert_eq!(severe.sample_at(severe.elapsed_ms).rain, 1.8);
    let change = clock.prepare(WeatherKind::StormMild as u8, 30_000).unwrap();
    clock.apply(change).unwrap();
    let before = clock.snapshot();
    let end = before.sample_at(before.transition_start_ms + 30_000);
    assert_eq!(end, WeatherKind::StormMild.values());
    assert!(before.sample_at(before.elapsed_ms).rain > 1.79);
    clock.finish().unwrap();
    drop(clock);
    let mut resumed = Clock::open(&root, 999).unwrap();
    assert_eq!(resumed.snapshot().to, WeatherKind::StormMild);
    assert_eq!(resumed.snapshot().from, before.from);
    resumed.finish().unwrap();
    drop(resumed);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn captured_weather_is_stable_and_override_invalidates_admitted_reads() {
    let root = temporary();
    let mut clock = Clock::open(&root, 7).unwrap();
    let capture = clock.capture();
    assert!(capture.weather.valid());
    // Clock progress is a captured historical input; explicit changes fence it.
    clock.started -= Duration::from_secs(2);
    assert!(capture.stamp.is_current());
    assert!(clock.capture().weather.elapsed_ms > capture.weather.elapsed_ms);
    let mut reads = crate::server::durable::TerrainReads::default();
    reads.weather = Some(capture.stamp);
    assert!(reads.keys().any(|key| key == state_key()));
    let change = clock.prepare(4, 0).unwrap();
    clock.apply(change).unwrap();
    assert!(!reads.is_current());
    assert_eq!(clock.capture().weather.rain_mm_h, 54.0);
    clock.finish().unwrap();
    drop(clock);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn natural_target_transitions_notify_once_even_when_the_target_kind_repeats() {
    use bloxgloom_host_api::gameplay::{Committed, Observer, ObserverRegistration, WeatherChanged};
    struct Witness(SyncSender<WeatherChanged>);
    impl Observer for Witness {
        fn on_commit(&self, _: &Committed) {}
        fn on_weather(&self, event: &WeatherChanged) {
            let _ = self.0.try_send(*event);
        }
    }
    let root = temporary();
    let mut clock = Clock::open(&root, 7).unwrap();
    let before = clock.published();
    clock.started -= Duration::from_millis(200_000);
    clock.last_publish -= Duration::from_secs(2);
    let current = clock.poll().unwrap();
    assert!(current.transition_start_ms > before.transition_start_ms);
    let (tx, rx) = mpsc::sync_channel(4);
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_gameplay_observer(ObserverRegistration {
            key: "witness:weather".into(),
            version: 1,
            observer: Arc::new(Witness(tx)),
        })
        .unwrap();
    let lane = crate::server::notifications::Lane::new(&catalog).unwrap();
    lane.weather(before, current);
    let event = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(event.current, current.observation(current.elapsed_ms));
    lane.weather(current, current);
    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
    // A new transition toward the same target still carries a distinct start.
    let mut repeated = current;
    repeated.transition_start_ms += 1;
    lane.weather(current, repeated);
    assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
    clock.finish().unwrap();
    drop(clock);
    std::fs::remove_dir_all(root).unwrap();
}

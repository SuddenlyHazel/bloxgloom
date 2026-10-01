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
    send(&mut other, other_epoch, other_inventory, 2);
    loop {
        if let ServerMessage::ActionResult {
            accepted: false, ..
        } = protocol::read_server(&mut other).unwrap()
        {
            break;
        }
    }
    send(&mut peer, epoch, inventory, 2);
    let changed = loop {
        if let ServerMessage::Weather { snapshot } = protocol::read_server(&mut peer).unwrap()
            && snapshot.to == WeatherKind::Storm
        {
            break snapshot;
        }
    };
    loop {
        if let ServerMessage::Weather { snapshot } = protocol::read_server(&mut other).unwrap()
            && snapshot.to == WeatherKind::Storm
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
    assert_eq!(resumed.to, WeatherKind::Storm);
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

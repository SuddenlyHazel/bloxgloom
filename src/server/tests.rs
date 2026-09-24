use super::*;
use std::fs;
use std::net::TcpListener;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn delayed_movement_cannot_cross_a_block() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("bloxgloom-server-{}-{stamp}", std::process::id()));
    let mut world = World::new(1, path.clone()).unwrap();
    world.edit(1, 40, 0, STONE).unwrap();
    let position = resolve_movement(&mut world, [0.5, 40.0, 0.5], [2.0, 0.0, 0.0]).unwrap();
    assert!(
        position[0] < 1.0,
        "player crossed a one-block wall: {position:?}"
    );
    drop(world);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn set_view_acknowledges_the_clamped_radius() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-view-distance-{}-{stamp}",
        std::process::id()
    ));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (sender, receiver) = mpsc::sync_channel(2);
    let id = 1;
    let mut state = State {
        world: World::new(7, path.clone()).unwrap(),
        seed: 7,
        clients: HashMap::from([(
            id,
            Client {
                sender,
                socket,
                sent: HashSet::new(),
                center: ChunkKey { x: 0, y: 0, z: 0 },
                radius: DEFAULT_VIEW,
                position: [0.5, 50.0, 0.5],
                last_move: Instant::now(),
                last_seq: 0,
            },
        )]),
        next_id: 2,
    };

    handle_message(&mut state, id, ClientMessage::SetView { radius: u8::MAX }).unwrap();
    assert_eq!(state.clients[&id].radius, MAX_VIEW_DISTANCE);
    assert!(matches!(
        receiver.recv().unwrap(),
        ServerMessage::ViewDistance {
            radius: MAX_VIEW_DISTANCE
        }
    ));

    handle_message(&mut state, id, ClientMessage::SetView { radius: 0 }).unwrap();
    assert_eq!(state.clients[&id].radius, MIN_VIEW_DISTANCE);
    assert!(matches!(
        receiver.recv().unwrap(),
        ServerMessage::ViewDistance {
            radius: MIN_VIEW_DISTANCE
        }
    ));

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn spawn_is_above_terrain_with_player_headroom() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for (index, seed) in [0, 1, 7, 0xB10C_6100, u64::MAX].into_iter().enumerate() {
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-spawn-{}-{stamp}-{index}",
            std::process::id()
        ));
        let mut world = World::new(seed, path.clone()).unwrap();
        let position = spawn_position(&mut world).unwrap();
        let feet_y = position[1] as i32;
        assert_eq!(world.get_block(0, feet_y, 0).unwrap(), AIR);
        assert_eq!(world.get_block(0, feet_y + 1, 0).unwrap(), AIR);
        assert_ne!(world.get_block(0, feet_y - 1, 0).unwrap(), AIR);
        for depth in 1..=7 {
            assert_ne!(
                world.get_block(0, feet_y - depth, 0).unwrap(),
                AIR,
                "new-world spawn must not sit above a cave"
            );
        }
        assert!(!collides(&mut world, position).unwrap());
        drop(world);
        fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn two_clients_share_edit_and_resync_stays_ordered() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("bloxgloom-wire-{}-{stamp}", std::process::id()));
    let shared = Arc::new(Mutex::new(State {
        world: World::new(7, path.clone()).unwrap(),
        seed: 7,
        clients: HashMap::new(),
        next_id: 1,
    }));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server_state = Arc::clone(&shared);
    let server = thread::spawn(move || {
        let mut workers = Vec::new();
        for _ in 0..2 {
            let (socket, _) = listener.accept().unwrap();
            let state = Arc::clone(&server_state);
            workers.push(thread::spawn(move || serve_client(socket, state).unwrap()));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    let mut socket = TcpStream::connect(address).unwrap();
    let mut peer = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    protocol::write_client(
        &mut socket,
        &ClientMessage::Hello {
            name: "Tester".into(),
        },
    )
    .unwrap();
    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "Peer".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        protocol::read_server(&mut socket).unwrap(),
        ServerMessage::Welcome { seed: 7, .. }
    ));
    let feet_y = match protocol::read_server(&mut socket).unwrap() {
        ServerMessage::Position { ack_seq: 0, y, .. } => y as i32,
        other => panic!("expected initial position, got {other:?}"),
    };
    assert!(matches!(
        protocol::read_server(&mut socket).unwrap(),
        ServerMessage::ViewDistance {
            radius: DEFAULT_VIEW
        }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Welcome { seed: 7, .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Position { ack_seq: 0, .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::ViewDistance {
            radius: DEFAULT_VIEW
        }
    ));
    let block_y = feet_y - 1;
    let (key, local) = world_to_chunk(0, block_y, 0);
    let version = (0..200)
        .find_map(|_| match protocol::read_server(&mut socket).unwrap() {
            ServerMessage::Chunk(chunk) if chunk.key == key => {
                assert_ne!(chunk.blocks[crate::world::Chunk::index(local).unwrap()], 0);
                Some(chunk.version)
            }
            _ => None,
        })
        .expect("target chunk snapshot");
    let peer_version = (0..200)
        .find_map(|_| match protocol::read_server(&mut peer).unwrap() {
            ServerMessage::Chunk(chunk) if chunk.key == key => Some(chunk.version),
            _ => None,
        })
        .expect("peer target chunk snapshot");
    assert_eq!(peer_version, version);
    protocol::write_client(
        &mut socket,
        &ClientMessage::Edit {
            x: 0,
            y: block_y,
            z: 0,
            block: GLOWSTONE,
        },
    )
    .unwrap();
    let new_version = (0..200)
        .find_map(|_| match protocol::read_server(&mut socket).unwrap() {
            ServerMessage::Delta {
                key: got,
                version,
                x,
                y,
                z,
                block,
            } if got == key => {
                assert_eq!([x as usize, y as usize, z as usize], local);
                assert_eq!(block, GLOWSTONE);
                Some(version)
            }
            _ => None,
        })
        .expect("durable edit delta");
    assert_eq!(new_version, version + 1);
    let peer_delta = (0..200)
        .find_map(|_| match protocol::read_server(&mut peer).unwrap() {
            ServerMessage::Delta {
                key: got,
                version,
                block,
                ..
            } if got == key => {
                assert_eq!(block, GLOWSTONE);
                Some(version)
            }
            _ => None,
        })
        .expect("peer edit delta");
    assert_eq!(peer_delta, new_version);
    protocol::write_client(&mut peer, &ClientMessage::Resync { key }).unwrap();
    let refreshed = (0..200)
        .find_map(|_| match protocol::read_server(&mut peer).unwrap() {
            ServerMessage::Chunk(chunk) if chunk.key == key && chunk.version == new_version => {
                Some(chunk)
            }
            _ => None,
        })
        .expect("resync snapshot");
    assert_eq!(
        refreshed.blocks[crate::world::Chunk::index(local).unwrap()],
        GLOWSTONE
    );
    drop(socket);
    drop(peer);
    server.join().unwrap();
    drop(shared);
    let restarted = Arc::new(Mutex::new(State {
        world: World::new(7, path.clone()).unwrap(),
        seed: 7,
        clients: HashMap::new(),
        next_id: 1,
    }));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server_state = Arc::clone(&restarted);
    let server = thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        serve_client(socket, server_state).unwrap();
    });
    let mut reconnect = TcpStream::connect(address).unwrap();
    reconnect
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    protocol::write_client(
        &mut reconnect,
        &ClientMessage::Hello {
            name: "Returning".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        protocol::read_server(&mut reconnect).unwrap(),
        ServerMessage::Welcome { seed: 7, .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut reconnect).unwrap(),
        ServerMessage::Position { ack_seq: 0, .. }
    ));
    let persisted = (0..200)
        .find_map(|_| match protocol::read_server(&mut reconnect).unwrap() {
            ServerMessage::Chunk(chunk) if chunk.key == key => Some(chunk),
            _ => None,
        })
        .expect("persisted chunk after restart");
    assert_eq!(persisted.version, new_version);
    assert_eq!(
        persisted.blocks[crate::world::Chunk::index(local).unwrap()],
        GLOWSTONE
    );
    drop(reconnect);
    server.join().unwrap();
    drop(restarted);
    fs::remove_dir_all(path).unwrap();
}

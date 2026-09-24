use super::*;
use std::fs;
use std::net::TcpListener;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn drop_snapshot_filter_ignores_age_but_not_authoritative_changes() {
    let original = DroppedItem {
        id: 1,
        item: 2,
        count: 3,
        position: [1.0, 2.0, 3.0],
        age_ms: 20,
    };
    let mut changed = original;
    changed.age_ms = 40;
    assert!(same_drop_positions(&[original], &[changed]));
    changed.position[1] -= 0.5;
    assert!(!same_drop_positions(&[original], &[changed]));
}

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
        inventory_store: InventoryStore::new(&path).unwrap(),
        drops: Drops::new(),
        seed: 7,
        clients: HashMap::from([(
            id,
            Client {
                profile: 1,
                inventory: Inventory::default(),
                last_drops_revision: u64::MAX,
                last_drop_anchor: [i32::MAX; 3],
                last_sent_drops: Vec::new(),
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
        last_drop_step: Instant::now(),
        last_drop_save: Instant::now(),
        moving_drops_dirty: false,
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
fn client_with_different_content_catalog_is_rejected_before_joining() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-content-handshake-{}-{stamp}",
        std::process::id()
    ));
    let shared = Arc::new(Mutex::new(State {
        world: World::new(7, path.clone()).unwrap(),
        inventory_store: InventoryStore::new(&path).unwrap(),
        drops: Drops::new(),
        seed: 7,
        clients: HashMap::new(),
        next_id: 1,
        last_drop_step: Instant::now(),
        last_drop_save: Instant::now(),
        moving_drops_dirty: false,
    }));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let state = Arc::clone(&shared);
    let server = thread::spawn(move || serve_client(socket, state));
    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "Outdated".into(),
            profile: 1,
            content_fingerprint: crate::content::catalog().fingerprint() ^ 1,
        },
    )
    .unwrap();
    assert_eq!(
        server.join().unwrap().unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert!(shared.lock().unwrap().clients.is_empty());
    drop(peer);
    drop(shared);
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
fn planted_flower_has_no_collision_and_falls_with_its_soil() {
    use crate::world::{RED_FLOWER, YELLOW_FLOWER};

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("bloxgloom-plants-{}-{stamp}", std::process::id()));
    let mut world = World::new(7, path.clone()).unwrap();
    let position = spawn_position(&mut world).unwrap();
    let y = position[1] as i32;
    assert!(supports_plant(world.get_block(0, y - 1, 0).unwrap()));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (sender, receiver) = mpsc::sync_channel(16);
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack {
        item: RED_FLOWER,
        count: 1,
    });
    let mut state = State {
        world,
        inventory_store: InventoryStore::new(&path).unwrap(),
        drops: Drops::open(&path).unwrap(),
        seed: 7,
        clients: HashMap::from([(
            1,
            Client {
                profile: 42,
                inventory,
                last_drops_revision: u64::MAX,
                last_drop_anchor: [i32::MAX; 3],
                last_sent_drops: Vec::new(),
                sender,
                socket,
                sent: HashSet::from([world_to_chunk(0, y, 0).0, world_to_chunk(0, y - 1, 0).0]),
                center: world_to_chunk(0, y, 0).0,
                radius: DEFAULT_VIEW,
                position,
                last_move: Instant::now(),
                last_seq: 0,
            },
        )]),
        next_id: 2,
        last_drop_step: Instant::now(),
        last_drop_save: Instant::now(),
        moving_drops_dirty: false,
    };
    handle_message(
        &mut state,
        1,
        ClientMessage::Edit {
            x: 0,
            y,
            z: 0,
            block: RED_FLOWER,
            slot: 0,
        },
    )
    .unwrap();
    assert_eq!(state.world.get_block(0, y, 0).unwrap(), RED_FLOWER);
    assert!(!collides(&mut state.world, position).unwrap());
    assert_eq!(state.clients[&1].inventory.slots[0], None);
    state.clients.get_mut(&1).unwrap().inventory.slots[0] = Some(crate::inventory::Stack {
        item: YELLOW_FLOWER,
        count: 1,
    });
    handle_message(
        &mut state,
        1,
        ClientMessage::Edit {
            x: 0,
            y,
            z: 0,
            block: YELLOW_FLOWER,
            slot: 0,
        },
    )
    .unwrap();
    assert_eq!(state.world.get_block(0, y, 0).unwrap(), YELLOW_FLOWER);
    assert_eq!(state.clients[&1].inventory.slots[0], None);
    assert_eq!(state.drops.nearby(position).len(), 1);
    handle_message(
        &mut state,
        1,
        ClientMessage::Edit {
            x: 0,
            y: y - 1,
            z: 0,
            block: AIR,
            slot: 0,
        },
    )
    .unwrap();
    assert_eq!(state.world.get_block(0, y, 0).unwrap(), AIR);
    assert_eq!(state.drops.nearby(position).len(), 3);
    assert!(receiver.try_iter().any(|message| matches!(message, ServerMessage::Delta { y: local_y, block: AIR, .. } if local_y as usize == world_to_chunk(0, y, 0).1[1])));
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn breaking_pickup_and_placement_are_server_owned_and_persisted() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("bloxgloom-items-{}-{stamp}", std::process::id()));
    let mut world = World::new(7, path.clone()).unwrap();
    let position = spawn_position(&mut world).unwrap();
    let y = position[1] as i32 - 1;
    let original = world.get_block(0, y, 0).unwrap();
    assert_ne!(original, AIR);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (sender, receiver) = mpsc::sync_channel(32);
    let store = InventoryStore::new(&path).unwrap();
    let mut state = State {
        world,
        inventory_store: store,
        drops: Drops::open(&path).unwrap(),
        seed: 7,
        clients: HashMap::from([(
            1,
            Client {
                profile: 42,
                inventory: Inventory::default(),
                last_drops_revision: u64::MAX,
                last_drop_anchor: [i32::MAX; 3],
                last_sent_drops: Vec::new(),
                sender,
                socket,
                sent: HashSet::new(),
                center: world_to_chunk(0, y, 0).0,
                radius: DEFAULT_VIEW,
                position,
                last_move: Instant::now(),
                last_seq: 0,
            },
        )]),
        next_id: 2,
        last_drop_step: Instant::now(),
        last_drop_save: Instant::now(),
        moving_drops_dirty: false,
    };
    handle_message(
        &mut state,
        1,
        ClientMessage::Edit {
            x: 0,
            y: y + 2,
            z: 0,
            block: original,
            slot: 0,
        },
    )
    .unwrap();
    assert_eq!(state.clients[&1].inventory.slots[0], None);
    handle_message(
        &mut state,
        1,
        ClientMessage::Edit {
            x: 0,
            y,
            z: 0,
            block: AIR,
            slot: 0,
        },
    )
    .unwrap();
    assert_eq!(state.world.get_block(0, y, 0).unwrap(), AIR);
    assert_eq!(state.drops.nearby(position).len(), 1);
    assert_eq!(Drops::open(&path).unwrap().nearby(position).len(), 1);
    thread::sleep(Duration::from_millis(300));
    collect_nearby(&mut state, 1).unwrap();
    assert!(receiver.try_iter().any(|message| matches!(
        message,
        ServerMessage::Pickups { items } if items.len() == 1 && items[0].count == 1
    )));
    assert_eq!(
        state.clients[&1].inventory.slots[0],
        Some(crate::inventory::Stack {
            item: original,
            count: 1
        })
    );
    assert!(state.drops.nearby(position).is_empty());
    assert_eq!(
        state.inventory_store.load(42).unwrap(),
        state.clients[&1].inventory
    );
    handle_message(
        &mut state,
        1,
        ClientMessage::Edit {
            x: 0,
            y,
            z: 0,
            block: original,
            slot: 0,
        },
    )
    .unwrap();
    assert_eq!(state.world.get_block(0, y, 0).unwrap(), original);
    assert_eq!(state.clients[&1].inventory.slots[0], None);
    assert_eq!(
        state.inventory_store.load(42).unwrap(),
        state.clients[&1].inventory
    );
    // A nearly full inventory takes only the free space, leaving the rest in-world.
    let other = if original == STONE { 1 } else { STONE };
    let mut almost_full = Inventory::default();
    for slot in &mut almost_full.slots {
        *slot = Some(crate::inventory::Stack {
            item: other,
            count: crate::inventory::STACK_LIMIT,
        });
    }
    almost_full.slots[0] = Some(crate::inventory::Stack {
        item: original,
        count: 127,
    });
    state.clients.get_mut(&1).unwrap().inventory = almost_full;
    state.drops.spawn(position, original, 10, Duration::ZERO);
    state.drops.save().unwrap();
    let _ = receiver.try_iter().count();
    collect_nearby(&mut state, 1).unwrap();
    assert_eq!(state.clients[&1].inventory.slots[0].unwrap().count, 128);
    assert_eq!(state.drops.nearby(position)[0].count, 9);
    assert!(receiver.try_iter().any(|message| matches!(
        message,
        ServerMessage::Pickups { items } if items.len() == 1 && items[0].count == 1
    )));
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn non_placeable_foliage_loot_is_picked_up_and_persisted() {
    use crate::items::{SAPLING, SEEDS, STICK};

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-foliage-loot-{}-{stamp}",
        std::process::id()
    ));
    let mut world = World::new(7, path.clone()).unwrap();
    let position = spawn_position(&mut world).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (sender, receiver) = mpsc::sync_channel(16);
    let mut state = State {
        world,
        inventory_store: InventoryStore::new(&path).unwrap(),
        drops: Drops::open(&path).unwrap(),
        seed: 7,
        clients: HashMap::from([(
            1,
            Client {
                profile: 42,
                inventory: Inventory::default(),
                last_drops_revision: u64::MAX,
                last_drop_anchor: [i32::MAX; 3],
                last_sent_drops: Vec::new(),
                sender,
                socket,
                sent: HashSet::from([world_to_chunk(0, position[1] as i32, 0).0]),
                center: world_to_chunk(0, position[1] as i32, 0).0,
                radius: DEFAULT_VIEW,
                position,
                last_move: Instant::now(),
                last_seq: 0,
            },
        )]),
        next_id: 2,
        last_drop_step: Instant::now(),
        last_drop_save: Instant::now(),
        moving_drops_dirty: false,
    };
    for item in [SEEDS, SAPLING, STICK] {
        state.drops.spawn(position, item, 1, Duration::ZERO);
    }
    state.drops.save().unwrap();
    assert_eq!(Drops::open(&path).unwrap().nearby(position).len(), 3);
    collect_nearby(&mut state, 1).unwrap();
    assert!(state.drops.nearby(position).is_empty());
    let inventory = &state.clients[&1].inventory;
    for item in [SEEDS, SAPLING, STICK] {
        assert!(
            inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == item)
        );
    }
    assert_eq!(state.inventory_store.load(42).unwrap(), *inventory);
    assert!(receiver.try_iter().any(|message| matches!(
        message,
        ServerMessage::Pickups { items } if items.len() == 3
    )));
    let feet_y = position[1] as i32;
    handle_message(
        &mut state,
        1,
        ClientMessage::Edit {
            x: 0,
            y: feet_y,
            z: 0,
            block: SEEDS,
            slot: 0,
        },
    )
    .unwrap();
    assert_eq!(state.world.get_block(0, feet_y, 0).unwrap(), AIR);
    assert!(
        receiver
            .try_iter()
            .any(|message| matches!(message, ServerMessage::EditRejected { .. }))
    );
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
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
        inventory_store: InventoryStore::new(&path).unwrap(),
        drops: Drops::new(),
        seed: 7,
        clients: HashMap::new(),
        next_id: 1,
        last_drop_step: Instant::now(),
        last_drop_save: Instant::now(),
        moving_drops_dirty: false,
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
            profile: 1,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
    )
    .unwrap();
    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "Peer".into(),
            profile: 2,
            content_fingerprint: crate::content::catalog().fingerprint(),
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
        protocol::read_server(&mut socket).unwrap(),
        ServerMessage::Inventory { .. }
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
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Inventory { .. }
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
            block: AIR,
            slot: 0,
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
                assert_eq!(block, AIR);
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
                assert_eq!(block, AIR);
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
        AIR
    );
    drop(socket);
    drop(peer);
    server.join().unwrap();
    drop(shared);
    let restarted = Arc::new(Mutex::new(State {
        world: World::new(7, path.clone()).unwrap(),
        inventory_store: InventoryStore::new(&path).unwrap(),
        drops: Drops::new(),
        seed: 7,
        clients: HashMap::new(),
        next_id: 1,
        last_drop_step: Instant::now(),
        last_drop_save: Instant::now(),
        moving_drops_dirty: false,
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
            profile: 1,
            content_fingerprint: crate::content::catalog().fingerprint(),
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
        AIR
    );
    drop(reconnect);
    server.join().unwrap();
    drop(restarted);
    fs::remove_dir_all(path).unwrap();
}

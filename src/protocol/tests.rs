use super::*;

#[test]
fn client_messages_round_trip() {
    let messages = [
        ClientMessage::Hello {
            name: "Miner".into(),
            profile: 123,
        },
        ClientMessage::Move {
            seq: 42,
            dx: 0.25,
            dy: -1.0,
            dz: 2.0,
        },
        ClientMessage::Edit {
            x: -17,
            y: 12,
            z: 31,
            block: 2,
            slot: 4,
        },
        ClientMessage::Resync {
            key: ChunkKey { x: -2, y: 0, z: 5 },
        },
        ClientMessage::SetView { radius: 6 },
        ClientMessage::Ping { nonce: u64::MAX },
        ClientMessage::InventoryMove {
            from: 1,
            to: 35,
            count: 64,
        },
        ClientMessage::DropStack { slot: 3, count: 2 },
    ];
    for message in messages {
        let mut bytes = Vec::new();
        write_client(&mut bytes, &message).unwrap();
        assert_eq!(read_client(bytes.as_slice()).unwrap(), message);
    }
}

#[test]
fn snapshot_and_delta_round_trip() {
    let key = ChunkKey { x: -2, y: 3, z: 4 };
    let chunk = Chunk {
        key,
        version: 99,
        blocks: vec![3; BLOCK_COUNT],
    };
    let mut bytes = Vec::new();
    write_server(&mut bytes, &ServerMessage::Chunk(chunk.clone())).unwrap();
    match read_server(bytes.as_slice()).unwrap() {
        ServerMessage::Chunk(decoded) => assert_eq!(decoded, chunk),
        other => panic!("unexpected message: {other:?}"),
    }
    bytes.clear();
    write_server(
        &mut bytes,
        &ServerMessage::Delta {
            key,
            version: 100,
            x: 15,
            y: 0,
            z: 4,
            block: 0,
        },
    )
    .unwrap();
    match read_server(bytes.as_slice()).unwrap() {
        ServerMessage::Delta {
            key: got,
            version,
            x,
            y,
            z,
            block,
        } => {
            assert_eq!((got, version, x, y, z, block), (key, 100, 15, 0, 4, 0));
        }
        other => panic!("unexpected message: {other:?}"),
    }
}

#[test]
fn view_distance_ack_round_trip_and_validation() {
    let message = ServerMessage::ViewDistance {
        radius: MAX_VIEW_DISTANCE,
    };
    let mut bytes = Vec::new();
    write_server(&mut bytes, &message).unwrap();
    assert!(matches!(
        read_server(bytes.as_slice()).unwrap(),
        ServerMessage::ViewDistance {
            radius: MAX_VIEW_DISTANCE
        }
    ));

    assert_eq!(
        write_server(
            Vec::new(),
            &ServerMessage::ViewDistance {
                radius: MIN_VIEW_DISTANCE - 1,
            },
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::InvalidData
    );
    let mut malformed = Vec::new();
    frame(&mut malformed, &[WIRE_VERSION, 7, MAX_VIEW_DISTANCE + 1]).unwrap();
    assert_eq!(
        read_server(malformed.as_slice()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn rejects_oversized_and_malformed_frames_before_allocating_payload() {
    let mut oversized = ((MAX_FRAME + 1) as u32).to_le_bytes().to_vec();
    assert_eq!(
        read_client(oversized.as_slice()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    oversized.clear();
    oversized.extend(4u32.to_le_bytes());
    oversized.extend([WIRE_VERSION, 5, 1, 0]);
    assert_eq!(
        read_client(oversized.as_slice()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn inventory_and_drop_snapshots_round_trip_with_bounds() {
    let mut slots = [None; SLOTS];
    slots[0] = Some(Stack {
        block: 3,
        count: 128,
    });
    slots[35] = Some(Stack { block: 8, count: 1 });
    let mut bytes = Vec::new();
    write_server(
        &mut bytes,
        &ServerMessage::Inventory {
            revision: 12,
            slots,
        },
    )
    .unwrap();
    match read_server(bytes.as_slice()).unwrap() {
        ServerMessage::Inventory {
            revision,
            slots: got,
        } => {
            assert_eq!(revision, 12);
            assert_eq!(got, slots);
        }
        other => panic!("unexpected {other:?}"),
    }
    bytes.clear();
    let items = vec![DroppedItem {
        id: 99,
        block: 2,
        count: 63,
        position: [-4.5, 7.0, 9.25],
        age_ms: 90_327,
    }];
    write_server(
        &mut bytes,
        &ServerMessage::Drops {
            revision: 5,
            items: items.clone(),
        },
    )
    .unwrap();
    match read_server(bytes.as_slice()).unwrap() {
        ServerMessage::Drops {
            revision,
            items: got,
        } => {
            assert_eq!(revision, 5);
            assert_eq!(got, items);
        }
        other => panic!("unexpected {other:?}"),
    }
    bytes.clear();
    write_server(
        &mut bytes,
        &ServerMessage::Pickups {
            items: items.clone(),
        },
    )
    .unwrap();
    match read_server(bytes.as_slice()).unwrap() {
        ServerMessage::Pickups { items: got } => assert_eq!(got, items),
        other => panic!("unexpected {other:?}"),
    }
    let mut bad = [None; SLOTS];
    bad[0] = Some(Stack {
        block: 1,
        count: 129,
    });
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::Inventory {
                revision: 1,
                slots: bad
            }
        )
        .is_err()
    );
}

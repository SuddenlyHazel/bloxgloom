use super::*;

#[test]
fn package_reload_protocol_roundtrips_and_rejects_invalid_flags_and_oversized_status() {
    let mut bytes = Vec::new();
    write_client(&mut bytes, &ClientMessage::ReloadPackages).unwrap();
    assert_eq!(
        read_client(bytes.as_slice()).unwrap(),
        ClientMessage::ReloadPackages
    );
    for reconnect in [false, true] {
        let message = ServerMessage::PackageReload {
            reconnect,
            text: "Reload rejected: café".into(),
        };
        let mut bytes = Vec::new();
        write_server(&mut bytes, &message).unwrap();
        assert_eq!(bytes.len(), server_wire_len(&message));
        assert!(
            matches!(read_server(bytes.as_slice()).unwrap(), ServerMessage::PackageReload { reconnect: actual, text } if actual == reconnect && text == "Reload rejected: café")
        );
        bytes[6] = 2;
        assert!(read_server(bytes.as_slice()).is_err());
    }
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::PackageReload {
                reconnect: false,
                text: "x".repeat(4097)
            }
        )
        .is_err()
    );
}

#[test]
fn sprint_wire_checks_bool_identity_and_estimated_frame_size() {
    for sprinting in [false, true] {
        let request = ClientMessage::SetSprinting { sprinting };
        let mut bytes = Vec::new();
        write_client(&mut bytes, &request).unwrap();
        assert_eq!(read_client(bytes.as_slice()).unwrap(), request);
        *bytes.last_mut().unwrap() = 2;
        assert!(read_client(bytes.as_slice()).is_err());
        let mut bytes = Vec::new();
        let response = ServerMessage::PlayerSprint {
            entity_id: (1 << 63) | 5,
            sprinting,
        };
        write_server(&mut bytes, &response).unwrap();
        assert_eq!(bytes.len(), server_wire_len(&response));
        assert!(
            matches!(read_server(bytes.as_slice()).unwrap(), ServerMessage::PlayerSprint { entity_id, sprinting: actual } if entity_id == (1 << 63) | 5 && actual == sprinting)
        );
        *bytes.last_mut().unwrap() = 2;
        assert!(read_server(bytes.as_slice()).is_err());
    }
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::PlayerSprint {
                entity_id: 5,
                sprinting: true
            }
        )
        .is_err()
    );
}

#[test]
fn flight_mode_and_jump_wire_validate_boolean_and_frame_size() {
    for flying in [false, true] {
        let request = ClientMessage::SetFlying { flying };
        let mut bytes = Vec::new();
        write_client(&mut bytes, &request).unwrap();
        assert_eq!(read_client(bytes.as_slice()).unwrap(), request);
        *bytes.last_mut().unwrap() = 2;
        assert!(read_client(bytes.as_slice()).is_err());
        let response = ServerMessage::FlyingMode { flying };
        let mut bytes = Vec::new();
        write_server(&mut bytes, &response).unwrap();
        assert_eq!(bytes.len(), server_wire_len(&response));
        assert!(
            matches!(read_server(bytes.as_slice()).unwrap(), ServerMessage::FlyingMode { flying: actual } if actual == flying)
        );
        *bytes.last_mut().unwrap() = 2;
        assert!(read_server(bytes.as_slice()).is_err());
    }
    let mut bytes = Vec::new();
    write_client(&mut bytes, &ClientMessage::Jump).unwrap();
    assert_eq!(read_client(bytes.as_slice()).unwrap(), ClientMessage::Jump);
}

#[test]
fn crouch_wire_round_trips_and_rejects_invalid_boolean_or_avatar_identity() {
    for crouching in [false, true] {
        let request = ClientMessage::SetCrouching { crouching };
        let mut bytes = Vec::new();
        write_client(&mut bytes, &request).unwrap();
        assert_eq!(read_client(bytes.as_slice()).unwrap(), request);
        *bytes.last_mut().unwrap() = 2;
        assert!(read_client(bytes.as_slice()).is_err());
        let mut bytes = Vec::new();
        let response = ServerMessage::PlayerStance {
            entity_id: (1 << 63) | 5,
            crouching,
        };
        write_server(&mut bytes, &response).unwrap();
        assert!(
            matches!(read_server(bytes.as_slice()).unwrap(), ServerMessage::PlayerStance { entity_id, crouching: actual } if entity_id == (1 << 63) | 5 && actual == crouching)
        );
        assert_eq!(bytes.len(), server_wire_len(&response));
        *bytes.last_mut().unwrap() = 2;
        assert!(read_server(bytes.as_slice()).is_err());
    }
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::PlayerStance {
                entity_id: 5,
                crouching: true
            }
        )
        .is_err()
    );
}

#[test]
fn mossbun_spawn_wire_has_only_an_authenticated_action_identity() {
    let message = ClientMessage::AdminSpawnEntity {
        entity_type: crate::content::MOSSBUN_ENTITY_TYPE,
        action_id: (1u128 << 64) | 1,
    };
    let mut wire = Vec::new();
    write_client(&mut wire, &message).unwrap();
    assert_eq!(read_client(wire.as_slice()).unwrap(), message);
    assert!(
        write_client(
            &mut Vec::new(),
            &ClientMessage::AdminSpawnEntity {
                action_id: 0,
                entity_type: crate::content::MOSSBUN_ENTITY_TYPE
            }
        )
        .is_err()
    );
}

#[test]
fn admin_grant_wire_round_trips_and_rejects_invalid_counts() {
    let item = crate::items::STICK;
    let message = ClientMessage::AdminGive {
        action_id: (1u128 << 64) | 1,
        item,
        count: 128,
    };
    let mut wire = Vec::new();
    write_client(&mut wire, &message).unwrap();
    assert_eq!(read_client(wire.as_slice()).unwrap(), message);
    assert!(
        write_client(
            &mut Vec::new(),
            &ClientMessage::AdminGive {
                action_id: (1u128 << 64) | 1,
                item,
                count: 129,
            }
        )
        .is_err()
    );
}

fn catalog_with_many_states(count: usize) -> Catalog {
    let mut catalog = Catalog::builtins();
    let mut block = catalog
        .block_type(crate::content::BlockTypeId(3))
        .unwrap()
        .clone();
    block.id = crate::content::BlockTypeId(16);
    block.key = "test:palette".into();
    block.properties = vec![crate::content::PropertyDef {
        name: "variant".into(),
        values: (0..count)
            .map(|index| format!("v{index:03}").into())
            .collect(),
    }];
    catalog.register_block(block).unwrap();
    for index in 0..count {
        catalog
            .register_state(
                BlockStateId(100_000 + index as u32),
                crate::content::BlockTypeId(16),
                vec![("variant".into(), format!("v{index:03}"))],
                None,
            )
            .unwrap();
    }
    catalog
}

#[test]
fn outbound_wire_lengths_match_serialized_frames() {
    let key = ChunkKey { x: -2, y: 3, z: 4 };
    let drop = DroppedItem {
        id: 7,
        item: crate::items::STICK,
        count: 2,
        components: None,
        position: [1.0, 2.0, 3.0],
        age_ms: 12,
    };
    let messages = vec![
        ServerMessage::BundleOffer {
            identity: BundleIdentity {
                client_runtime: crate::protocol::CLIENT_RUNTIME_VERSION,
                key: crate::server::client_bundle::CacheKey::from_bytes([7; 32]),
                total_len: 3,
            },
        },
        ServerMessage::BundlePart {
            offset: 0,
            bytes: vec![1, 2, 3],
        },
        ServerMessage::WorldTime {
            elapsed_ms: crate::daylight::INITIAL_MS,
        },
        ServerMessage::Welcome { id: 1, seed: 2 },
        ServerMessage::Position {
            ack_seq: 3,
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        ServerMessage::Chunk(Chunk::from_blocks(
            key,
            4,
            vec![crate::world::AIR; BLOCK_COUNT],
        )),
        ServerMessage::Delta {
            key,
            version: 5,
            x: 1,
            y: 2,
            z: 3,
            block: crate::world::AIR,
        },
        ServerMessage::EditRejected {
            reason: "no".into(),
        },
        ServerMessage::ActionResult {
            action_id: (1u128 << 64) | 6,
            accepted: false,
            reason: "retry".into(),
        },
        ServerMessage::ActionSession {
            epoch: 2,
            next_seq: 1,
            acked_seq: 0,
        },
        ServerMessage::ActionDeferred {
            action_id: (1u128 << 64) | 7,
        },
        ServerMessage::Pong { nonce: 7 },
        ServerMessage::ViewDistance { radius: 3 },
        ServerMessage::Inventory {
            revision: 8,
            slots: std::array::from_fn(|_| None),
        },
        ServerMessage::Drops {
            revision: 9,
            items: vec![drop.clone()],
        },
        ServerMessage::Pickups {
            items: vec![drop.clone()],
        },
        ServerMessage::OwnedEntity { id: 42 },
    ];
    for message in messages {
        let mut bytes = Vec::new();
        write_server(&mut bytes, &message).unwrap();
        assert_eq!(server_wire_len(&message), bytes.len(), "{message:?}");
    }
}

#[test]
fn owned_entity_identity_is_nonzero_and_round_trips() {
    let mut wire = Vec::new();
    write_server(&mut wire, &ServerMessage::OwnedEntity { id: 77 }).unwrap();
    assert!(matches!(
        read_server(wire.as_slice()).unwrap(),
        ServerMessage::OwnedEntity { id: 77 }
    ));
    assert!(write_server(Vec::new(), &ServerMessage::OwnedEntity { id: 0 }).is_err());
}

#[test]
fn public_entity_snapshot_and_grouped_commit_are_bounded_wire_records() {
    let catalog = Catalog::builtins();
    let key = ChunkKey { x: 3, y: -2, z: 7 };
    let chunk = Chunk::from_blocks(key, 4, vec![crate::world::AIR; BLOCK_COUNT]);
    let entity = PublicEntity {
        id: 19,
        entity_type: crate::content::EntityTypeId(3),
        revision: 2,
        motion_revision: 0,
        location: PublicEntityLocation::Anchored {
            anchor: [48, -32, 112],
            anchor_state: crate::content::KILN_DEFAULT_STATE,
        },
        // Only the registered public kiln view is sent; private slots are absent.
        payload: vec![2, 1, 65],
    };
    let pages = vec![vec![entity.clone()]];
    let checksum = snapshot_checksum(&chunk, 9, 12, &pages, &catalog).unwrap();
    let messages = [
        ServerMessage::WorldSnapshotStart(WorldSnapshotStart {
            chunk,
            epoch: 9,
            entity_revision: 12,
            entity_page_count: 1,
            checksum,
        }),
        ServerMessage::EntitySnapshotPage(EntitySnapshotPage {
            key,
            epoch: 9,
            entity_revision: 12,
            page_index: 0,
            page_count: 1,
            checksum,
            entities: vec![entity.clone()],
        }),
        ServerMessage::WorldCommitPart(WorldCommitPart {
            commit_id: 5,
            part_index: 0,
            part_count: 1,
            key,
            epoch: 9,
            block_from: 4,
            block_to: 4,
            entity_from: 12,
            entity_to: 15,
            blocks: vec![],
            entities: vec![PublicEntityChange::Upsert(entity)],
        }),
    ];
    for message in messages {
        let mut wire = Vec::new();
        write_server_with_catalog(&mut wire, &message, &catalog).unwrap();
        assert_eq!(server_wire_len(&message), wire.len());
        assert!(wire.len() <= MAX_FRAME + 4);
        let decoded = read_server_with_catalog(wire.as_slice(), &catalog).unwrap();
        assert_eq!(server_wire_len(&decoded), wire.len());
    }
}

#[test]
fn client_messages_round_trip() {
    let messages = [
        ClientMessage::Hello {
            name: "Miner".into(),
            profile: 123,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
        ClientMessage::Move {
            seq: 42,
            dx: 0.25,
            dy: -1.0,
            dz: 2.0,
        },
        ClientMessage::Edit {
            action_id: (1u128 << 64) | 0x1234,
            x: -17,
            y: 12,
            z: 31,
            block: crate::world::DIRT,
            slot: 4,
        },
        ClientMessage::Resync {
            key: ChunkKey { x: -2, y: 0, z: 5 },
        },
        ClientMessage::SetView { radius: 6 },
        ClientMessage::Ping { nonce: u64::MAX },
        ClientMessage::InventoryMove {
            action_id: (1u128 << 64) | 0x1235,
            from: 1,
            to: 35,
            count: 64,
        },
        ClientMessage::DropStack {
            action_id: (1u128 << 64) | 0x1236,
            slot: 3,
            count: 2,
        },
        ClientMessage::EntityInteract {
            action_id: (1u128 << 64) | 0x1237,
            target: [-16, 8, 2],
            payload: vec![1, 0, 1, 4, 2, 0],
        },
        ClientMessage::ActionAck {
            epoch: 3,
            through_seq: 8,
        },
    ];
    for message in messages {
        let mut bytes = Vec::new();
        write_client(&mut bytes, &message).unwrap();
        assert_eq!(read_client(bytes.as_slice()).unwrap(), message);
    }
}

#[test]
fn entity_interaction_rejects_empty_oversized_and_truncated_payloads() {
    let mut message = ClientMessage::EntityInteract {
        action_id: (1u128 << 64) | 1,
        target: [-1, 2, 3],
        payload: vec![1],
    };
    let mut bytes = Vec::new();
    write_client(&mut bytes, &message).unwrap();
    assert!(read_client(&bytes[..bytes.len() - 1]).is_err());
    if let ClientMessage::EntityInteract { payload, .. } = &mut message {
        payload.clear();
    }
    assert!(write_client(Vec::new(), &message).is_err());
    if let ClientMessage::EntityInteract { payload, .. } = &mut message {
        payload.resize(MAX_ENTITY_INTERACT_BYTES + 1, 1);
    }
    assert!(write_client(Vec::new(), &message).is_err());
}

#[test]
fn snapshot_and_delta_round_trip() {
    let key = ChunkKey { x: -2, y: 3, z: 4 };
    let chunk = Chunk::from_blocks(key, 99, vec![crate::world::STONE; BLOCK_COUNT]);
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
            block: crate::world::AIR,
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
            assert_eq!(
                (got, version, x, y, z, block),
                (key, 100, 15, 0, 4, crate::world::AIR)
            );
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
    let mut slots = std::array::from_fn(|_| None);
    slots[0] = Some(Stack::new(ItemId(3), 128));
    slots[35] = Some(Stack::new(crate::items::SEEDS, 1));
    let mut bytes = Vec::new();
    write_server(
        &mut bytes,
        &ServerMessage::Inventory {
            revision: 12,
            slots: slots.clone(),
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
        item: crate::items::STICK,
        count: 63,
        components: None,
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
    let mut bad = std::array::from_fn(|_| None);
    bad[0] = Some(Stack::new(ItemId(1), 129));
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

#[test]
fn separate_item_ids_round_trip_but_cannot_be_sent_as_block_edits() {
    for item in [
        crate::items::SEEDS,
        crate::items::SAPLING,
        crate::items::STICK,
    ] {
        let drop = DroppedItem {
            id: 1,
            item,
            count: 1,
            components: None,
            position: [0.0; 3],
            age_ms: 0,
        };
        let mut bytes = Vec::new();
        write_server(
            &mut bytes,
            &ServerMessage::Pickups {
                items: vec![drop.clone()],
            },
        )
        .unwrap();
        assert!(
            matches!(read_server(bytes.as_slice()).unwrap(), ServerMessage::Pickups { items } if items == [drop])
        );
        bytes[16] = 0;
        assert!(read_server(bytes.as_slice()).is_err());
        assert!(
            write_client(
                Vec::new(),
                &ClientMessage::Edit {
                    action_id: (1u128 << 64) | 1,
                    x: 0,
                    y: 0,
                    z: 0,
                    block: BlockStateId(item.0),
                    slot: 0
                }
            )
            .is_err()
        );
    }
    for invalid_item in [0, 16, 127, 131, 255] {
        let drop = DroppedItem {
            id: 1,
            item: ItemId(invalid_item),
            count: 1,
            components: None,
            position: [0.0; 3],
            age_ms: 0,
        };
        assert!(
            write_server(
                Vec::new(),
                &ServerMessage::Pickups {
                    items: vec![drop.clone()]
                }
            )
            .is_err()
        );
    }
}

#[test]
fn action_receipts_round_trip_and_reject_invalid_ids() {
    for message in [
        ServerMessage::ActionResult {
            action_id: (1u128 << 64) | 42,
            accepted: true,
            reason: String::new(),
        },
        ServerMessage::ActionResult {
            action_id: (1u128 << 64) | 43,
            accepted: false,
            reason: "out of reach".into(),
        },
    ] {
        let mut bytes = Vec::new();
        write_server(&mut bytes, &message).unwrap();
        match (read_server(bytes.as_slice()).unwrap(), message) {
            (
                ServerMessage::ActionResult {
                    action_id: got_id,
                    accepted: got_accepted,
                    reason: got_reason,
                },
                ServerMessage::ActionResult {
                    action_id,
                    accepted,
                    reason,
                },
            ) => assert_eq!(
                (got_id, got_accepted, got_reason),
                (action_id, accepted, reason)
            ),
            _ => panic!("wrong action result"),
        }
    }
    assert!(
        write_client(
            Vec::new(),
            &ClientMessage::DropStack {
                action_id: 0,
                slot: 0,
                count: 1
            }
        )
        .is_err()
    );
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::ActionResult {
                action_id: 0,
                accepted: true,
                reason: String::new()
            }
        )
        .is_err()
    );
    let mut malformed = Vec::new();
    let mut payload = vec![WIRE_VERSION, 11];
    payload.extend(((1u128 << 64) | 9).to_le_bytes());
    payload.extend([2, 0]);
    frame(&mut malformed, &payload).unwrap();
    assert_eq!(
        read_server(malformed.as_slice()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    for message in [
        ServerMessage::ActionSession {
            epoch: 2,
            next_seq: 9,
            acked_seq: 8,
        },
        ServerMessage::ActionDeferred {
            action_id: (2u128 << 64) | 9,
        },
    ] {
        let mut bytes = Vec::new();
        write_server(&mut bytes, &message).unwrap();
        assert!(matches!(
            (&message, read_server(bytes.as_slice()).unwrap()),
            (
                ServerMessage::ActionSession { .. },
                ServerMessage::ActionSession { .. }
            ) | (
                ServerMessage::ActionDeferred { .. },
                ServerMessage::ActionDeferred { .. }
            )
        ));
    }
    assert!(
        write_client(
            Vec::new(),
            &ClientMessage::ActionAck {
                epoch: 0,
                through_seq: 1
            }
        )
        .is_err()
    );
    assert!(
        write_client(
            Vec::new(),
            &ClientMessage::InventoryMove {
                action_id: 1,
                from: 0,
                to: 1,
                count: 1
            }
        )
        .is_err()
    );
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::ActionSession {
                epoch: 1,
                next_seq: 1,
                acked_seq: 1
            }
        )
        .is_err()
    );
}

#[test]
fn wide_palette_indices_upgrade_at_257_and_cover_full_chunk() {
    for (unique, expected_width) in [(255, 1), (256, 1), (257, 2), (4_096, 2)] {
        let blocks = (0..BLOCK_COUNT)
            .map(|index| BlockStateId(65_536 + (index % unique) as u32))
            .collect::<Vec<_>>();
        assert_eq!(PalettedBlocks::from(blocks.clone()).unique_states(), unique);
        let mut bytes = vec![WIRE_VERSION, 3];
        write_palette(&mut bytes, &blocks).unwrap();
        assert_eq!(bytes[4], expected_width);
        let mut cursor = Cursor::new(&bytes);
        assert_eq!(read_palette_with(&mut cursor, |_| true).unwrap(), blocks);
        cursor.done().unwrap();
        let expected = 2 + 2 + 1 + unique * 4 + BLOCK_COUNT * expected_width as usize;
        assert_eq!(bytes.len(), expected);
        assert!(bytes.len() < MAX_FRAME);
    }
}

#[test]
fn resident_palette_mutation_keeps_wire_length_exact_without_rescanning() {
    let key = ChunkKey { x: 7, y: -2, z: 1 };
    let mut blocks = PalettedBlocks::uniform(crate::world::AIR);
    for index in 0..257 {
        blocks.set(index, BlockStateId(100_000 + index as u32));
    }
    let message = ServerMessage::Chunk(Chunk {
        key,
        version: 4,
        blocks,
    });
    let mut encoded = Vec::new();
    let catalog = catalog_with_many_states(257);
    write_server_with_catalog(&mut encoded, &message, &catalog).unwrap();
    assert_eq!(server_wire_len(&message), encoded.len());
    let expected = match &message {
        ServerMessage::Chunk(chunk) => chunk.clone(),
        _ => unreachable!(),
    };
    assert!(
        matches!(read_server_with_catalog(encoded.as_slice(), &catalog).unwrap(), ServerMessage::Chunk(decoded) if decoded == expected)
    );
}

#[test]
fn palette_rejects_duplicate_unknown_and_out_of_range_indices() {
    let blocks = vec![BlockStateId(65_536); BLOCK_COUNT];
    let mut bytes = vec![WIRE_VERSION, 3];
    write_palette(&mut bytes, &blocks).unwrap();
    let mut cursor = Cursor::new(&bytes);
    assert!(read_palette_with(&mut cursor, |_| false).is_err());
    let mut out_of_range = bytes.clone();
    *out_of_range.last_mut().unwrap() = 1;
    let mut cursor = Cursor::new(&out_of_range);
    assert!(read_palette_with(&mut cursor, |_| true).is_err());
    let mut truncated = bytes;
    truncated.pop();
    let mut cursor = Cursor::new(&truncated);
    assert!(read_palette_with(&mut cursor, |_| true).is_err());
}

#[test]
fn content_parts_and_ready_round_trip_with_bounds() {
    let part = ServerMessage::ContentManifestPart {
        fingerprint: 73,
        total_len: 6,
        offset: 2,
        bytes: vec![1, 2, 3, 4],
    };
    let mut bytes = Vec::new();
    write_server(&mut bytes, &part).unwrap();
    assert_eq!(server_wire_len(&part), bytes.len());
    assert!(matches!(read_server(bytes.as_slice()).unwrap(),
        ServerMessage::ContentManifestPart { fingerprint: 73, total_len: 6, offset: 2, bytes } if bytes == [1, 2, 3, 4]));
    let mut ready = Vec::new();
    write_client(&mut ready, &ClientMessage::ContentReady { fingerprint: 73 }).unwrap();
    assert_eq!(
        read_client(ready.as_slice()).unwrap(),
        ClientMessage::ContentReady { fingerprint: 73 }
    );
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::ContentManifestPart {
                fingerprint: 73,
                total_len: 6,
                offset: 4,
                bytes: vec![1, 2, 3, 4],
            }
        )
        .is_err()
    );
}

#[test]
fn remapped_ids_above_65535_survive_wire_v8() {
    let local = Catalog::builtins();
    let mut manifest = crate::content::ContentManifest::from_catalog(&local);
    for entry in &mut manifest.entries {
        match (entry.kind, entry.key.as_str()) {
            (b'B', "bloxgloom:stone") => entry.id = 65_536,
            (b'S', "bloxgloom:stone") => entry.id = 65_537,
            (b'I', "bloxgloom:stone") => entry.id = 65_538,
            _ => {}
        }
    }
    manifest
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    let catalog = manifest.resolve_catalog(&local).unwrap();
    let state = BlockStateId(65_537);
    let item = ItemId(65_538);
    let key = ChunkKey { x: -1, y: 2, z: 3 };
    let chunk = Chunk::from_blocks(key, 11, vec![state; BLOCK_COUNT]);
    let mut bytes = Vec::new();
    write_server_with_catalog(&mut bytes, &ServerMessage::Chunk(chunk.clone()), &catalog).unwrap();
    assert!(
        matches!(read_server_with_catalog(bytes.as_slice(), &catalog).unwrap(), ServerMessage::Chunk(got) if got == chunk)
    );
    bytes.clear();
    let edit = ClientMessage::Edit {
        action_id: (1u128 << 64) | 9,
        x: -1,
        y: 2,
        z: 3,
        block: state,
        slot: 0,
    };
    write_client_with_catalog(&mut bytes, &edit, &catalog).unwrap();
    assert_eq!(
        read_client_with_catalog(bytes.as_slice(), &catalog).unwrap(),
        edit
    );
    bytes.clear();
    let mut slots = std::array::from_fn(|_| None);
    slots[0] = Some(Stack::with_components(item, 128, 2, vec![7, 8, 9]).unwrap());
    write_server_with_catalog(
        &mut bytes,
        &ServerMessage::Inventory {
            revision: 4,
            slots: slots.clone(),
        },
        &catalog,
    )
    .unwrap();
    assert!(
        matches!(read_server_with_catalog(bytes.as_slice(), &catalog).unwrap(), ServerMessage::Inventory { revision: 4, slots: got } if got == slots)
    );
    let mut oversized_component = bytes.clone();
    oversized_component[22..24].copy_from_slice(&((MAX_COMPONENT_BYTES + 1) as u16).to_le_bytes());
    assert!(read_server_with_catalog(oversized_component.as_slice(), &catalog).is_err());
    bytes.clear();
    let drop = DroppedItem {
        id: 1,
        item,
        count: 1,
        components: None,
        position: [0.0; 3],
        age_ms: 1,
    };
    write_server_with_catalog(
        &mut bytes,
        &ServerMessage::Pickups {
            items: vec![drop.clone()],
        },
        &catalog,
    )
    .unwrap();
    assert!(
        matches!(read_server_with_catalog(bytes.as_slice(), &catalog).unwrap(), ServerMessage::Pickups { items } if items == [drop])
    );
}

#[test]
fn committed_fire_cues_round_trip_with_a_strict_cell_bound() {
    let cells = vec![[i32::MIN, 8, i32::MAX], [2, 80, -3]];
    let mut wire = Vec::new();
    write_server(
        &mut wire,
        &ServerMessage::FireBursts {
            cells: cells.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        wire.len(),
        server_wire_len(&ServerMessage::FireBursts {
            cells: cells.clone()
        })
    );
    assert!(
        matches!(read_server(wire.as_slice()).unwrap(), ServerMessage::FireBursts { cells: read } if read == cells)
    );
    assert!(write_server(Vec::new(), &ServerMessage::FireBursts { cells: vec![] }).is_err());
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::FireBursts {
                cells: vec![[0; 3]; MAX_FIRE_BURSTS + 1]
            }
        )
        .is_err()
    );
    let mut invalid = wire;
    invalid[6] = 0; // 4-byte prefix, version, tag, count
    assert!(read_server(invalid.as_slice()).is_err());
}

#[test]
fn world_time_wire_wrap_boundary_and_invalid_samples() {
    for elapsed_ms in [
        0,
        crate::daylight::INITIAL_MS,
        crate::daylight::CYCLE_MS - 1,
    ] {
        let mut wire = Vec::new();
        write_server(&mut wire, &ServerMessage::WorldTime { elapsed_ms }).unwrap();
        assert!(matches!(read_server(wire.as_slice()).unwrap(),
            ServerMessage::WorldTime { elapsed_ms: decoded } if decoded == elapsed_ms));
        // Reject out-of-range clocks received from the network as well as sent locally.
        wire[6..14].copy_from_slice(&crate::daylight::CYCLE_MS.to_le_bytes());
        assert!(read_server(wire.as_slice()).is_err());
    }
    assert!(
        write_server(
            &mut Vec::new(),
            &ServerMessage::WorldTime {
                elapsed_ms: crate::daylight::CYCLE_MS,
            }
        )
        .is_err()
    );
}

#[test]
fn set_world_time_request_round_trips_and_rejects_invalid_phase() {
    for elapsed_ms in [
        0,
        crate::daylight::INITIAL_MS,
        crate::daylight::CYCLE_MS - 1,
    ] {
        let message = ClientMessage::SetWorldTime {
            action_id: (1 << 64) | 1,
            elapsed_ms,
        };
        let mut wire = Vec::new();
        write_client(&mut wire, &message).unwrap();
        assert_eq!(read_client(wire.as_slice()).unwrap(), message);
        wire[22..30].copy_from_slice(&crate::daylight::CYCLE_MS.to_le_bytes());
        assert!(read_client(wire.as_slice()).is_err());
    }
    assert!(
        write_client(
            &mut Vec::new(),
            &ClientMessage::SetWorldTime {
                action_id: (1 << 64) | 1,
                elapsed_ms: crate::daylight::CYCLE_MS,
            }
        )
        .is_err()
    );
}

#[test]
fn player_roster_round_trips_exact_sessions_and_rejects_noncanonical_membership() {
    let player = PlayerSummary {
        profile: u128::MAX,
        session: u64::MAX,
        name: "Alice".into(),
    };
    let message = ServerMessage::PlayerRoster {
        revision: 1,
        players: vec![player.clone()],
    };
    let mut wire = Vec::new();
    write_server(&mut wire, &message).unwrap();
    assert_eq!(wire.len(), server_wire_len(&message));
    let ServerMessage::PlayerRoster { revision, players } = read_server(wire.as_slice()).unwrap()
    else {
        panic!("expected roster");
    };
    assert_eq!(revision, 1);
    assert_eq!(players, vec![player.clone()]);
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::PlayerRoster {
                revision: 1,
                players: vec![player.clone(), player]
            }
        )
        .is_err()
    );
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::PlayerRoster {
                revision: 1,
                players: vec![PlayerSummary {
                    profile: 1,
                    session: 0,
                    name: "Alice".into()
                }]
            }
        )
        .is_err()
    );
}

#[test]
fn player_state_snapshots_preserve_binary_projection_and_bound_service_keys() {
    let state = PlayerState {
        key: format!("demo:{}", "a".repeat(64)),
        revision: u64::MAX,
        public: vec![0, 255, 1],
    };
    let message = ServerMessage::PlayerStates {
        profile: u128::MAX,
        session: u64::MAX,
        snapshot: 1,
        states: vec![state.clone()],
    };
    let mut wire = Vec::new();
    write_server(&mut wire, &message).unwrap();
    assert_eq!(wire.len(), server_wire_len(&message));
    let ServerMessage::PlayerStates {
        profile,
        session,
        snapshot,
        states,
    } = read_server(wire.as_slice()).unwrap()
    else {
        panic!("expected states");
    };
    assert_eq!((profile, session, snapshot), (u128::MAX, u64::MAX, 1));
    assert_eq!(states, vec![state.clone()]);
    for states in [
        vec![state.clone(), state.clone()],
        vec![PlayerState {
            public: vec![0; 1025],
            ..state.clone()
        }],
        vec![PlayerState {
            key: "missing-colon".into(),
            ..state.clone()
        }],
        vec![state; 129],
    ] {
        assert!(
            write_server(
                Vec::new(),
                &ServerMessage::PlayerStates {
                    profile: 1,
                    session: 1,
                    snapshot: 1,
                    states
                }
            )
            .is_err()
        );
    }
    for (profile, session, snapshot) in [(0, 1, 1), (1, 0, 1), (1, 1, 0)] {
        assert!(
            write_server(
                Vec::new(),
                &ServerMessage::PlayerStates {
                    profile,
                    session,
                    snapshot,
                    states: vec![]
                }
            )
            .is_err()
        );
    }
    for end in 0..wire.len() {
        assert!(read_server(&wire[..end]).is_err());
    }
}

#[test]
fn player_notices_preserve_exact_session_and_reject_invalid_text_and_flags() {
    for kicked in [false, true] {
        let message = ServerMessage::PlayerNotice {
            profile: u128::MAX,
            session: u64::MAX,
            kicked,
            text: "é".repeat(127) + "!",
        };
        let mut wire = Vec::new();
        write_server(&mut wire, &message).unwrap();
        assert_eq!(wire.len(), server_wire_len(&message));
        let ServerMessage::PlayerNotice {
            profile,
            session,
            kicked: actual,
            text,
        } = read_server(wire.as_slice()).unwrap()
        else {
            panic!("expected notice")
        };
        assert_eq!(
            (profile, session, actual, text.len()),
            (u128::MAX, u64::MAX, kicked, 255)
        );
        for end in 0..wire.len() {
            assert!(read_server(&wire[..end]).is_err());
        }
        // Frame length, version, tag, exact profile and epoch precede the strict bool.
        wire[4 + 2 + 16 + 8] = 2;
        assert!(read_server(wire.as_slice()).is_err());
    }
    for (profile, session, text) in [
        (0, 1, "ok".into()),
        (1, 0, "ok".into()),
        (1, 1, String::new()),
        (1, 1, "x".repeat(256)),
        (1, 1, "bad\nreason".into()),
        (1, 1, "bad\u{0085}reason".into()),
    ] {
        assert!(
            write_server(
                Vec::new(),
                &ServerMessage::PlayerNotice {
                    profile,
                    session,
                    kicked: false,
                    text
                }
            )
            .is_err()
        );
    }
}

#[test]
fn teleport_and_ready_frames_preserve_full_width_identities_and_strict_bounds() {
    let mut wire = Vec::new();
    write_server(
        &mut wire,
        &ServerMessage::PlayerTeleport {
            profile: u128::MAX,
            session: u64::MAX,
            reset: u64::MAX,
            position: [-999999.5, 300.0, 999999.5],
        },
    )
    .unwrap();
    let message = read_server(wire.as_slice()).unwrap();
    assert_eq!(wire.len(), server_wire_len(&message));
    assert!(matches!(
        message,
        ServerMessage::PlayerTeleport {
            profile: u128::MAX,
            session: u64::MAX,
            reset: u64::MAX,
            position: [-999999.5, 300.0, 999999.5]
        }
    ));
    for end in 0..wire.len() {
        assert!(read_server(&wire[..end]).is_err());
    }
    for (profile, session, reset, position) in [
        (0, 1, 1, [0.0; 3]),
        (1, 0, 1, [0.0; 3]),
        (1, 1, 0, [0.0; 3]),
        (1, 1, 1, [f32::NAN, 0.0, 0.0]),
        (1, 1, 1, [1_000_000.0, 0.0, 0.0]),
    ] {
        assert!(
            write_server(
                Vec::new(),
                &ServerMessage::PlayerTeleport {
                    profile,
                    session,
                    reset,
                    position
                }
            )
            .is_err()
        );
    }
    let ready = ClientMessage::MovementReady {
        session: u64::MAX,
        reset: u64::MAX,
        next_seq: u64::MAX - 1,
    };
    let mut wire = Vec::new();
    write_client(&mut wire, &ready).unwrap();
    assert_eq!(read_client(wire.as_slice()).unwrap(), ready);
    for (session, reset, next_seq) in [(0, 1, 1), (1, 0, 1), (1, 1, 0), (1, 1, u64::MAX)] {
        assert!(
            write_client(
                Vec::new(),
                &ClientMessage::MovementReady {
                    session,
                    reset,
                    next_seq
                }
            )
            .is_err()
        );
    }
}

#[path = "tests/character.rs"]
mod character;

#[test]
fn committed_action_spawn_mappings_roundtrip_and_reject_invalid_ordinals() {
    let action_id = (1u128 << 64) | 1;
    let spawned = vec![
        SpawnReceipt {
            ordinal: 0,
            entity: u64::MAX >> 1,
        },
        SpawnReceipt {
            ordinal: 1,
            entity: 17,
        },
    ];
    let response = ServerMessage::ActionSpawned {
        action_id,
        spawned: spawned.clone(),
    };
    let mut bytes = vec![];
    write_server(&mut bytes, &response).unwrap();
    let ServerMessage::ActionSpawned {
        action_id: actual,
        spawned: values,
    } = read_server(bytes.as_slice()).unwrap()
    else {
        panic!("wrong reply")
    };
    assert_eq!(actual, action_id);
    assert_eq!(values, spawned);
    assert_eq!(bytes.len(), server_wire_len(&response));
    for values in [
        vec![],
        vec![SpawnReceipt {
            ordinal: 1,
            entity: 1,
        }],
        vec![SpawnReceipt {
            ordinal: 0,
            entity: 0,
        }],
        vec![SpawnReceipt {
            ordinal: 0,
            entity: 1 << 63,
        }],
        vec![
            SpawnReceipt {
                ordinal: 0,
                entity: 1,
            },
            SpawnReceipt {
                ordinal: 1,
                entity: 1,
            },
        ],
    ] {
        assert!(
            write_server(
                Vec::new(),
                &ServerMessage::ActionSpawned {
                    action_id,
                    spawned: values
                }
            )
            .is_err()
        );
    }
    for length in 0..bytes.len() {
        assert!(read_server(&bytes[..length]).is_err());
    }
}

#[test]
fn weather_wire_round_trip_rejects_invalid_physical_values() {
    let mut snapshot = crate::weather::WeatherSnapshot::initial(123);
    snapshot.to = crate::weather::WeatherKind::StormSevere;
    snapshot.from = snapshot.to.values();
    snapshot.transition_duration_ms = 30_000;
    let message = ServerMessage::Weather { snapshot };
    let mut bytes = Vec::new();
    write_server(&mut bytes, &message).unwrap();
    let ServerMessage::Weather { snapshot: decoded } = read_server(&mut bytes.as_slice()).unwrap()
    else {
        panic!("expected weather snapshot");
    };
    assert_eq!(decoded, snapshot);
    snapshot.from.rain = f32::NAN;
    assert!(write_server(&mut Vec::new(), &ServerMessage::Weather { snapshot }).is_err());
}

#[test]
fn sound_batch_roundtrip_length_and_truncated_frames_are_checked() {
    use bloxgloom_host_api::sound::{Event, Kind};
    let events = vec![
        Event {
            owner: "demo".into(),
            voice: "run".into(),
            kind: Kind::Play {
                clip: "demo:motor".into(),
                position: [1.0, 2.0, 3.0],
                entity: Some(u64::MAX),
                gain: 0.5,
                pitch: 1.25,
                looping: true,
            },
        },
        Event {
            owner: "demo".into(),
            voice: "run".into(),
            kind: Kind::Update {
                position: Some([2.0; 3]),
                gain: 1.0,
                pitch: 0.5,
            },
        },
        Event {
            owner: "demo".into(),
            voice: "run".into(),
            kind: Kind::Stop,
        },
    ];
    let message = ServerMessage::Sounds { id: 7, events };
    let mut bytes = Vec::new();
    write_server(&mut bytes, &message).unwrap();
    assert_eq!(bytes.len(), server_wire_len(&message));
    let ServerMessage::Sounds { id, events } = read_server(bytes.as_slice()).unwrap() else {
        panic!("expected sounds")
    };
    let ServerMessage::Sounds {
        id: expected,
        events: expected_events,
    } = message
    else {
        unreachable!()
    };
    assert_eq!(id, expected);
    assert_eq!(events, expected_events);
    for len in 0..bytes.len() {
        assert!(read_server(&bytes[..len]).is_err());
    }
    assert!(
        write_server(
            Vec::new(),
            &ServerMessage::Sounds {
                id: 0,
                events: vec![]
            }
        )
        .is_err()
    );
}

use super::*;
use crate::protocol::PublicEntityChange;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

#[test]
fn remote_player_spawn_move_and_leave_publish_ordered_entity_changes() {
    let save = TestSave::new("player-entity-publication");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let observer = join(&mut state, &mut tick, 1);
    let key = state.clients[&observer.id].center;
    for _ in 0..100 {
        if state.clients[&observer.id].sent.contains(&key) {
            break;
        }
        let _ = messages(&observer);
        run_empty_tick(&mut state, &mut tick);
    }
    assert!(state.clients[&observer.id].sent.contains(&key));
    let _ = messages(&observer);

    let actor = join(&mut state, &mut tick, 2);
    let entity_id = state
        .player_entities
        .id_for_session(actor.id)
        .unwrap()
        .get();
    assert!(messages(&observer).iter().any(|message| matches!(
        message,
        ServerMessage::WorldCommitPart(part)
            if part.key == key && part.entities.iter().any(|change| matches!(
                change,
                PublicEntityChange::Upsert(entity) if entity.id == entity_id
            ))
    )));

    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Command {
            id: actor.id,
            sequence: 1,
            message: ClientMessage::Move {
                seq: 1,
                dx: 0.1,
                dy: 0.0,
                dz: 0.0,
            },
        }],
    );
    assert!(messages(&observer).iter().any(|message| matches!(
        message,
        ServerMessage::WorldCommitPart(part)
            if part.key == key && part.entities.iter().any(|change| matches!(
                change,
                PublicEntityChange::Upsert(entity)
                    if entity.id == entity_id && entity.motion_revision > 1
            ))
    )));

    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Leave {
            id: actor.id,
            sequence: 2,
        }],
    );
    assert!(state.player_entities.id_for_session(actor.id).is_none());
    assert!(messages(&observer).iter().any(|message| matches!(
        message,
        ServerMessage::WorldCommitPart(part)
            if part.key == key && part.entities.iter().any(|change| matches!(
                change,
                PublicEntityChange::Remove { id, .. } if *id == entity_id
            ))
    )));

    let rejoined = join(&mut state, &mut tick, 2);
    assert_ne!(
        state
            .player_entities
            .id_for_session(rejoined.id)
            .unwrap()
            .get(),
        entity_id
    );
}

#[test]
fn view_radius_is_clamped_and_acknowledged_by_the_coordinator() {
    let save = TestSave::new("view-radius");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 1);
    let _ = messages(&session);

    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Command {
            id: session.id,
            sequence: 1,
            message: ClientMessage::SetView { radius: u8::MAX },
        }],
    );
    assert_eq!(state.clients[&session.id].radius, MAX_VIEW_DISTANCE);
    assert!(messages(&session).iter().any(|message| matches!(
        message,
        ServerMessage::ViewDistance {
            radius: MAX_VIEW_DISTANCE
        }
    )));

    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Command {
            id: session.id,
            sequence: 2,
            message: ClientMessage::SetView { radius: 0 },
        }],
    );
    assert_eq!(state.clients[&session.id].radius, MIN_VIEW_DISTANCE);
    assert!(messages(&session).iter().any(|message| matches!(
        message,
        ServerMessage::ViewDistance {
            radius: MIN_VIEW_DISTANCE
        }
    )));
}

#[test]
fn streamed_interest_pins_release_on_resync_and_disconnect() {
    let save = TestSave::new("interest-pins");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 1);
    for _ in 0..100 {
        if !state.clients[&session.id].sent.is_empty() {
            break;
        }
        let _ = messages(&session);
        run_empty_tick(&mut state, &mut tick);
    }
    let subscribed = state.clients[&session.id].sent.len();
    assert!(subscribed > 0);
    assert_eq!(state.world.pinned_chunk_count(), subscribed);
    let key = *state.clients[&session.id].sent.iter().next().unwrap();
    handle_message(&mut state, session.id, ClientMessage::Resync { key }).unwrap();
    assert!(!state.clients[&session.id].sent.contains(&key));
    assert_eq!(state.world.pinned_chunk_count(), subscribed - 1);
    state.remove_client(session.id);
    assert_eq!(state.world.pinned_chunk_count(), 0);
}

#[test]
fn full_outbound_queue_disconnects_only_the_slow_client() {
    let save = TestSave::new("slow-client-isolation");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let slow = join(&mut state, &mut tick, 1);
    let healthy = join(&mut state, &mut tick, 2);
    let _ = messages(&healthy);

    let (sender, held_receiver) = state.outbound.client_queue_with_limits(1, 2 * 1024 * 1024);
    state.clients.get_mut(&slow.id).unwrap().sender = sender;
    assert!(state.clients[&slow.id].enqueue(ServerMessage::Pong { nonce: 1 }));
    spawn_drop(
        &mut state,
        tick,
        slow.joined.position,
        crate::items::ItemId::new(crate::world::STONE.get()),
        1,
        Duration::ZERO,
    );
    run_empty_tick(&mut state, &mut tick);

    assert!(!state.clients.contains_key(&slow.id));
    assert!(state.clients.contains_key(&healthy.id));
    assert!(
        messages(&healthy)
            .iter()
            .any(|message| matches!(message, ServerMessage::Drops { .. }))
    );
    drop(held_receiver);
}

#[test]
fn failed_startup_queue_does_not_register_a_ghost_profile() {
    let save = TestSave::new("closed-startup-queue");
    let mut state = state_for(&save, 7);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let _peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (sender, receiver) = state.outbound.client_queue();
    drop(receiver);

    let next_id = state.next_id;
    let error = join_client(&mut state, 99, 1, Inventory::default(), sender, &socket)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ErrorKind::BrokenPipe);
    assert_eq!(state.next_id, next_id);
    assert!(state.clients.is_empty());
}

#[test]
fn mismatched_catalog_is_rejected_before_a_join_enters_the_coordinator_queue() {
    let save = TestSave::new("content-handshake");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let store = InventoryStore::new(save.path()).unwrap();
    let (input, receiver) = mpsc::sync_channel(INPUT_CAPACITY);
    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "Outdated".into(),
            profile: 1,
            content_fingerprint: crate::content::catalog().fingerprint() ^ 1,
        },
    )
    .unwrap();
    protocol::write_client(
        &mut peer,
        &ClientMessage::ContentReady {
            fingerprint: crate::content::catalog().fingerprint() ^ 1,
        },
    )
    .unwrap();

    let error = net::serve_client(
        socket,
        store,
        input,
        Arc::new(OutboundTelemetry::default()),
        net::ContentHandshake::from_local_catalog().unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
    ));
}

#[test]
fn admission_limit_defaults_to_128_and_guards_256_without_allocating_clients() {
    let save = TestSave::new("admission-limit");
    assert_eq!(
        server_state(7, save.path().to_path_buf())
            .unwrap()
            .admission_limit,
        128
    );
    assert_eq!(
        server_state_with_limit(7, save.path().to_path_buf(), 256)
            .unwrap()
            .admission_limit,
        256
    );
    assert_eq!(
        server_state_with_limit(7, save.path().to_path_buf(), 0)
            .err()
            .unwrap()
            .kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        server_state_with_limit(7, save.path().to_path_buf(), 257)
            .err()
            .unwrap()
            .kind(),
        ErrorKind::InvalidInput
    );
}

#[test]
fn joined_players_spawn_above_solid_terrain_with_headroom() {
    for (index, seed) in [0, 7, u64::MAX].into_iter().enumerate() {
        let save = TestSave::new(&format!("spawn-{index}"));
        let mut state = state_for(&save, seed);
        let mut tick = 1;
        let session = join(&mut state, &mut tick, index as u128 + 1);
        let position = session.joined.position;
        let feet_y = position[1] as i32;
        assert_eq!(state.world.get_block(0, feet_y, 0).unwrap(), AIR);
        assert_eq!(state.world.get_block(0, feet_y + 1, 0).unwrap(), AIR);
        assert_ne!(state.world.get_block(0, feet_y - 1, 0).unwrap(), AIR);
        for depth in 1..=7 {
            assert_ne!(
                state.world.get_block(0, feet_y - depth, 0).unwrap(),
                AIR,
                "seed {seed} placed a new player above a cave"
            );
        }
        assert!(!collides(&mut state.world, position).unwrap());
    }
}

#[test]
fn joins_find_lower_safe_surface_after_origin_support_is_mined() {
    let save = TestSave::new("mined-spawn-support");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let first = join(&mut state, &mut tick, 1);
    let original_y = first.joined.position[1] as i32;
    assert_ne!(state.world.cached_block(0, original_y - 2, 0), Some(AIR));

    let output = command_and_wait(
        &mut state,
        &mut tick,
        &first,
        1,
        first.action_id(1),
        ClientMessage::Edit {
            action_id: first.action_id(1),
            x: 0,
            y: original_y - 1,
            z: 0,
            block: AIR,
            slot: 0,
        },
    );
    assert_eq!(action_result(&output, first.action_id(1)), Some(true));
    assert_eq!(state.world.cached_block(0, original_y - 1, 0), Some(AIR));

    let second = join(&mut state, &mut tick, 2);
    assert_eq!(second.joined.position[1] as i32, original_y - 1);
    assert!(!collides(&mut state.world, second.joined.position).unwrap());
}

#[test]
fn startup_and_live_spawn_can_use_negative_ground_after_excavation() {
    let save = TestSave::new("negative-origin-spawn");
    let mut state = state_for(&save, 7);
    let ceiling = crate::world::MAX_GENERATED_HEIGHT + 32;
    let mut edits = Vec::new();
    for y in crate::world::BEDROCK_Y + 1..ceiling {
        if state.world.get_block(0, y, 0).unwrap() != AIR {
            edits.push((0, y, 0, AIR));
        }
    }
    let prepared = state.world.prepare_edits(&edits).unwrap();
    state.world.apply_prepared_edits(prepared).unwrap();

    let expected_y = (crate::world::BEDROCK_Y + 1) as f32;
    assert_eq!(spawn_position(&mut state.world).unwrap()[1], expected_y);
    assert_eq!(spawn_position_cached(&mut state).unwrap()[1], expected_y);

    let mut tick = 1;
    let joined = join(&mut state, &mut tick, 9);
    assert_eq!(joined.joined.position[1], expected_y);
    assert!(!collides(&mut state.world, joined.joined.position).unwrap());
}

#[test]
fn multiple_clients_receive_edit_delta_then_resync_snapshot_in_order() {
    let save = TestSave::new("multi-client-resync");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let first = join(&mut state, &mut tick, 101);
    let second = join(&mut state, &mut tick, 102);
    let _ = messages(&first);
    let _ = messages(&second);
    let feet_y = first.joined.position[1] as i32;
    let block_y = feet_y - 1;
    let key = world_to_chunk(0, block_y, 0).0;
    let local = world_to_chunk(0, block_y, 0).1;
    let local_wire = local.map(|coordinate| coordinate as u8);
    let original_version = state.world.cached_version(key).unwrap();
    assert_ne!(state.world.cached_block(0, block_y, 0), Some(AIR));
    wait_for_subscription(&mut state, &mut tick, first.id, key);
    wait_for_subscription(&mut state, &mut tick, second.id, key);

    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Command {
            id: first.id,
            sequence: 1,
            message: ClientMessage::Edit {
                action_id: first.action_id(1),
                x: 0,
                y: block_y,
                z: 0,
                block: AIR,
                slot: 0,
            },
        }],
    );
    let mut first_output = Vec::new();
    let mut second_output = Vec::new();
    for _ in 0..1_000 {
        first_output.extend(messages(&first));
        second_output.extend(messages(&second));
        let first_delta = first_output.iter().any(|message| {
            matches!(
                message,
                ServerMessage::WorldCommitPart(part)
                    if part.key == key
                        && part.block_from == original_version
                        && part.block_to == original_version + 1
                        && part.blocks.iter().any(|cell| cell.local == local_wire && cell.block == AIR)
            )
        });
        let second_delta = second_output.iter().any(|message| {
            matches!(
                message,
                ServerMessage::WorldCommitPart(part)
                    if part.key == key
                        && part.block_from == original_version
                        && part.block_to == original_version + 1
                        && part.blocks.iter().any(|cell| cell.local == local_wire && cell.block == AIR)
            )
        });
        if first_delta
            && second_delta
            && action_result(&first_output, first.action_id(1)) == Some(true)
        {
            break;
        }
        run_empty_tick(&mut state, &mut tick);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(action_result(&first_output, first.action_id(1)), Some(true));
    assert!(first_output.iter().any(|message| matches!(
        message,
        ServerMessage::WorldCommitPart(part)
            if part.key == key
                && part.block_from == original_version
                && part.block_to == original_version + 1
                && part.blocks.iter().any(|cell| cell.local == local_wire && cell.block == AIR)
    )));
    assert!(second_output.iter().any(|message| matches!(
        message,
        ServerMessage::WorldCommitPart(part)
            if part.key == key
                && part.block_from == original_version
                && part.block_to == original_version + 1
                && part.blocks.iter().any(|cell| cell.local == local_wire && cell.block == AIR)
    )));

    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Command {
            id: second.id,
            sequence: 1,
            message: ClientMessage::Resync { key },
        }],
    );
    for _ in 0..500 {
        if let Some(chunk) = messages(&second)
            .into_iter()
            .find_map(|message| match message {
                ServerMessage::WorldSnapshotStart(start) if start.chunk.key == key => {
                    Some(start.chunk)
                }
                _ => None,
            })
        {
            assert_eq!(chunk.version, original_version + 1);
            assert_eq!(
                chunk.blocks[crate::world::Chunk::index(local).unwrap()],
                AIR
            );
            return;
        }
        run_empty_tick(&mut state, &mut tick);
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("resync did not publish an authoritative updated chunk");
}

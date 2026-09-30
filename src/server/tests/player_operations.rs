//! Late receipt publication must not act on a replacement connection.
use super::*;
use bloxgloom_host_api::gameplay::{PlayerOperation, PlayerOperationKind};

#[test]
fn committed_player_operations_never_follow_profile_to_replacement_session() {
    let save = TestSave::new("player-operation-replacement");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let first = join(&mut state, &mut tick, 17);
    state.remove_client(first.id);
    let replacement = join(&mut state, &mut tick, 17);
    assert!(replacement.action_epoch > first.action_epoch);
    let _ = messages(&replacement);
    for kind in [
        PlayerOperationKind::Message("Old notice".into()),
        PlayerOperationKind::Kick("Old removal".into()),
        PlayerOperationKind::Appearance([1, 2, 3]),
        PlayerOperationKind::Teleport([33.5, 300.0, 0.5]),
    ] {
        players::committed(
            &mut state,
            players::Published::operations(vec![PlayerOperation {
                profile: 17,
                session: first.action_epoch,
                kind,
            }])
            .unwrap(),
        )
        .unwrap();
        assert!(state.clients.contains_key(&replacement.id));
        assert_eq!(
            state.clients[&replacement.id].position(),
            replacement.joined.position
        );
        assert_eq!(
            state.player_entities.appearance_for_session(replacement.id),
            Some([0; 4])
        );
        assert!(
            !messages(&replacement)
                .iter()
                .any(|m| matches!(m, ServerMessage::PlayerNotice { .. }))
        );
    }
    players::committed(
        &mut state,
        players::Published::operations(vec![PlayerOperation {
            profile: 17,
            session: replacement.action_epoch,
            kind: PlayerOperationKind::Kick("Current removal".into()),
        }])
        .unwrap(),
    )
    .unwrap();
    assert!(!state.clients.contains_key(&replacement.id));
    assert!(
        !state
            .player_runtime
            .is_admitted(17, replacement.action_epoch)
    );
    assert!(messages(&replacement).iter().any(
        |m| matches!(m,ServerMessage::PlayerNotice {kicked:true,text,..} if text=="Current removal")
    ));
}

#[test]
fn successive_player_teleports_discard_old_moves_and_only_latest_reset_opens_input() {
    let save = TestSave::new("player-teleport-sequence");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 17);
    state
        .clients
        .get_mut(&session.id)
        .unwrap()
        .pending_moves
        .push_back(MovementCommand {
            seq: 5,
            delta: [0.1, 0.0, 0.0],
        });
    let mut resets = Vec::new();
    for position in [[33.5, 300.0, 0.5], [49.5, 300.0, 0.5]] {
        players::committed(
            &mut state,
            players::Published::operations(vec![PlayerOperation {
                profile: 17,
                session: session.action_epoch,
                kind: PlayerOperationKind::Teleport(position),
            }])
            .unwrap(),
        )
        .unwrap();
        let client = &state.clients[&session.id];
        assert_eq!(client.position(), position);
        assert!(client.pending_moves.is_empty() && client.movement_reset.pending);
        assert_eq!(client.movement.credit_nanoblocks(), 0);
        assert_eq!(
            client.center,
            world_to_chunk(position[0] as i32, position[1] as i32, position[2] as i32).0
        );
        let reset = messages(&session)
            .into_iter()
            .find_map(|m| {
                if let ServerMessage::PlayerTeleport { reset, .. } = m {
                    Some(reset)
                } else {
                    None
                }
            })
            .unwrap();
        resets.push(reset);
    }
    assert!(resets[1] > resets[0]);
    handle_message(
        &mut state,
        session.id,
        ClientMessage::MovementReady {
            session: session.action_epoch,
            reset: resets[0],
            next_seq: 6,
        },
    )
    .unwrap();
    handle_message(
        &mut state,
        session.id,
        ClientMessage::Move {
            seq: 6,
            dx: 0.1,
            dy: 0.0,
            dz: 0.0,
        },
    )
    .unwrap();
    assert!(state.clients[&session.id].pending_moves.is_empty());
    handle_message(
        &mut state,
        session.id,
        ClientMessage::MovementReady {
            session: session.action_epoch,
            reset: resets[1],
            next_seq: 7,
        },
    )
    .unwrap();
    assert!(!state.clients[&session.id].movement_reset.pending);
    assert_eq!(state.clients[&session.id].movement.last_seq(), 6);
    handle_message(
        &mut state,
        session.id,
        ClientMessage::Move {
            seq: 6,
            dx: 0.1,
            dy: 0.0,
            dz: 0.0,
        },
    )
    .unwrap();
    assert!(state.clients[&session.id].pending_moves.is_empty());
    handle_message(
        &mut state,
        session.id,
        ClientMessage::Move {
            seq: 7,
            dx: 0.1,
            dy: 0.0,
            dz: 0.0,
        },
    )
    .unwrap();
    assert_eq!(state.clients[&session.id].pending_moves.len(), 1);
    assert_eq!(
        state.position_store.load(17).unwrap(),
        Some([49.5, 300.0, 0.5])
    );
}

#[test]
fn failed_teleport_delivery_orders_avatar_motion_before_disconnect_removal() {
    let save = TestSave::new("player-teleport-slow-peer");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 17);
    let mut position = state.clients[&session.id].position();
    position[0] += 1.0;
    state.durability.publish_queue.clear();
    drop(session.receiver);
    players::committed(
        &mut state,
        players::Published::operations(vec![PlayerOperation {
            profile: 17,
            session: session.action_epoch,
            kind: PlayerOperationKind::Teleport(position),
        }])
        .unwrap(),
    )
    .unwrap();
    assert!(!state.clients.contains_key(&session.id));
    assert!(state.player_entities.id_for_session(session.id).is_none());
    assert_eq!(state.durability.publish_queue.len(), 2);
    assert!(matches!(
        &state.durability.publish_queue[0]
            .entity_commit
            .as_ref()
            .unwrap()
            .deltas[..],
        [EntityDelta::Moved(_)]
    ));
    assert!(matches!(
        &state.durability.publish_queue[1]
            .entity_commit
            .as_ref()
            .unwrap()
            .deltas[..],
        [EntityDelta::Despawned { .. }]
    ));
}

#[test]
fn committed_player_appearance_storage_failure_preserves_public_avatar_and_stops_mutation() {
    let save = TestSave::new("player-operation-appearance-failure");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 17);
    let path = save
        .path()
        .join("players/00000000000000000000000000000011.appearance");
    std::fs::create_dir(&path).unwrap();
    let operation = PlayerOperation {
        profile: 17,
        session: session.action_epoch,
        kind: PlayerOperationKind::Appearance([1, 2, 3]),
    };
    assert!(
        players::committed(
            &mut state,
            players::Published::operations(vec![operation]).unwrap()
        )
        .is_err()
    );
    assert!(state.durability.failed);
    assert_eq!(
        state.player_entities.appearance_for_session(session.id),
        Some([0; 4])
    );
    assert!(path.is_dir());
}

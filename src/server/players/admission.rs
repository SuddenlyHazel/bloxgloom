//! Admission installs a session only after the complete handshake is queued.
use super::super::*;

pub(in crate::server) fn join_named_client(
    state: &mut State,
    profile: u128,
    name: &str,
    action_epoch: u64,
    loaded_inventory: Inventory,
    sender: OutboundQueue,
    socket: &TcpStream,
) -> io::Result<JoinReply> {
    if state.durability.failed {
        return Err(io::Error::new(
            ErrorKind::ConnectionAborted,
            "durability subsystem failed; restart required",
        ));
    }
    if state.clients.len() >= state.admission_limit {
        return Err(io::Error::new(ErrorKind::ConnectionRefused, "server full"));
    }
    if state
        .clients
        .values()
        .any(|client| client.profile == profile)
    {
        return Err(io::Error::new(
            ErrorKind::AlreadyExists,
            "profile already connected",
        ));
    }
    if state.durability.profile_reserved(profile) {
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "profile has a pending durable action; retry join",
        ));
    }
    let inventory = match state.durability.inventory_overlay.get(&profile) {
        Some(inventory) => inventory.clone(),
        None => loaded_inventory,
    };
    let health = super::health::view(&state.system_runtime, profile)?;
    let health_cell = super::health::capture_profile(&state.system_runtime, profile)?;
    let health_state = bloxgloom_host_api::player_health::State::decode(&health_cell.state.data)
        .map_err(io::Error::other)?;
    let saved = state.position_store.load_with_life(profile)?;
    if saved.is_some_and(|value| value.1 > health_state.life) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "position life exceeds durable health life",
        ));
    }
    let recover_checkpoint = health_state
        .respawn
        .is_some_and(|(life, _)| saved.is_none_or(|value| value.1 < life));
    let saved = match health_state.respawn {
        Some((life, position)) if saved.is_none_or(|v| v.1 < life) => Some(position),
        _ => saved.map(|v| v.0),
    };
    let position = match saved {
        Some(saved) => match spawn::collides_cached(state, saved)? {
            Some(false) => saved,
            Some(true) => spawn_position_cached(state)?,
            None => {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "saved position chunks are loading",
                ));
            }
        },
        None => spawn_position_cached(state)?,
    };
    let appearance = state
        .appearance_store
        .load(profile, state.world.catalog())?;
    let position = super::admit(
        state,
        profile,
        name,
        action_epoch,
        position,
        &inventory,
        appearance.legacy(),
    )?;
    if recover_checkpoint {
        let life = health_state
            .respawn
            .expect("validated pending checkpoint")
            .0;
        state
            .position_store
            .save_with_life(profile, position, life)?;
    }
    let id = state.next_id;
    let next_id = crate::server::session_ids::next_after(id)?;
    let socket = socket.try_clone()?;
    let (owned_entity_id, spawn_delta) = state
        .player_entities
        .spawn_session_with_appearance(id, position, appearance)
        .map_err(io::Error::other)?;
    let center = world_to_chunk(
        position[0].floor() as i32,
        position[1].floor() as i32,
        position[2].floor() as i32,
    )
    .0;
    // Queue the complete handshake before registering the client. The
    // publish phase can otherwise enqueue terrain before Welcome while the
    // connection thread is waiting for this reply.
    for message in [
        ServerMessage::Welcome {
            id,
            seed: state.seed,
        },
        ServerMessage::PlayerHealth {
            profile,
            session: action_epoch,
            health,
        },
        ServerMessage::OwnedEntity {
            id: owned_entity_id.get(),
        },
        ServerMessage::ActionSession {
            epoch: action_epoch,
            next_seq: 1,
            acked_seq: 0,
        },
        ServerMessage::Position {
            ack_seq: 0,
            x: position[0],
            y: position[1],
            z: position[2],
        },
        ServerMessage::ViewDistance {
            radius: DEFAULT_VIEW,
        },
        ServerMessage::Inventory {
            revision: inventory.revision,
            slots: inventory.slots.clone(),
        },
        ServerMessage::WorldTime {
            elapsed_ms: state.world_time.now(),
        },
        ServerMessage::Weather {
            snapshot: state.weather.snapshot(),
        },
    ] {
        if sender.try_send(message).is_err() {
            state.player_entities.discard_session(id);
            return Err(io::Error::new(
                ErrorKind::BrokenPipe,
                "client startup queue closed",
            ));
        }
    }
    // Runtime posture snapshots follow the fixed handshake; never mutate the
    // saved avatar appearance to convey collision authority.
    let mut stances = state
        .clients
        .iter()
        .filter(|(_, client)| client.movement.crouching())
        .map(|(&session, _)| ServerMessage::PlayerStance {
            entity_id: crate::server::entities::EntityId::for_player_session(session)
                .unwrap()
                .get(),
            crouching: true,
        })
        .collect::<Vec<_>>();
    stances.sort_unstable_by_key(|message| match message {
        ServerMessage::PlayerStance { entity_id, .. } => *entity_id,
        _ => unreachable!(),
    });
    stances.push(ServerMessage::PlayerStance {
        entity_id: owned_entity_id.get(),
        crouching: false,
    });
    let mut sprints = state
        .clients
        .iter()
        .filter(|(_, client)| client.movement.sprinting())
        .map(|(&session, _)| ServerMessage::PlayerSprint {
            entity_id: crate::server::entities::EntityId::for_player_session(session)
                .unwrap()
                .get(),
            sprinting: true,
        })
        .collect::<Vec<_>>();
    sprints.sort_unstable_by_key(|message| match message {
        ServerMessage::PlayerSprint { entity_id, .. } => *entity_id,
        _ => unreachable!(),
    });
    sprints.push(ServerMessage::PlayerSprint {
        entity_id: owned_entity_id.get(),
        sprinting: false,
    });
    for message in stances.into_iter().chain(sprints) {
        if sender.try_send(message).is_err() {
            state.player_entities.discard_session(id);
            return Err(io::Error::new(
                ErrorKind::BrokenPipe,
                "stance startup queue closed",
            ));
        }
    }
    state.next_id = next_id;
    state.clients.insert(
        id,
        Client {
            name: name.into(),
            action_epoch,
            last_roster_revision: 0,
            last_player_state_revision: 0,
            last_player_states: None,
            profile,
            inventory,
            last_drops_revision: u64::MAX,
            last_drop_anchor: [i32::MAX; 3],
            last_sent_drops: Vec::new(),
            sender,
            socket,
            sent: Default::default(),
            sent_epochs: Default::default(),
            sent_block_versions: Default::default(),
            sent_entity_revisions: Default::default(),
            next_snapshot_epoch: 1,
            center,
            radius: DEFAULT_VIEW,
            movement: MovementState::new(position, 0),
            pending_moves: VecDeque::new(),
            movement_reset: Default::default(),
            health,
        },
    );
    if let Err(error) = state.queue_player_entity_deltas(vec![spawn_delta]) {
        state.player_entities.discard_session(id);
        state.clients.remove(&id);
        state.durability.failed = true;
        return Err(error);
    }
    Ok(JoinReply { id })
}

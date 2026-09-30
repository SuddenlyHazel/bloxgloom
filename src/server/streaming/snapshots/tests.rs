use super::*;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::server::durable::{self, BlockDelta, CommitAction, CommitBarrier};
use crate::server::outbound::{OUTBOUND_FRAME_CAPACITY, OutboundFrame};
use crate::server::{join_client, server_state, streaming};
use crate::world::{AIR, STONE, world_to_chunk};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

struct Fixture {
    state: State,
    // Fields drop in declaration order: worker pools stop before save cleanup.
    _save: Save,
}

struct Save(std::path::PathBuf);

impl Fixture {
    fn new(label: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-snapshots-{label}-{}-{stamp}",
            std::process::id()
        ));
        Self {
            state: server_state(7, path.clone()).unwrap(),
            _save: Save(path),
        }
    }

    fn join(&mut self, profile: u128) -> Session {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        let (sender, receiver) = self.state.outbound.client_queue();
        let id = join_client(
            &mut self.state,
            profile,
            1,
            Default::default(),
            sender,
            &socket,
        )
        .unwrap()
        .id;
        receiver.try_iter().for_each(drop);
        Session {
            id,
            receiver,
            _peer: peer,
        }
    }
}

impl Drop for Save {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Session {
    id: u64,
    receiver: Receiver<OutboundFrame>,
    _peer: TcpStream,
}

fn snapshots(session: &Session) -> Vec<OutboundFrame> {
    session
        .receiver
        .try_iter()
        .filter(|frame| {
            matches!(
                frame.message(),
                ServerMessage::WorldSnapshotStart(_) | ServerMessage::EntitySnapshotPage(_)
            )
        })
        .collect()
}

fn selection(state: &State, sessions: &[&Session], key: ChunkKey) -> Selection {
    let mut selected = Selection::default();
    for session in sessions {
        selected.select(session.id, &state.clients[&session.id], key);
    }
    selected
}

fn stage_edit(state: &mut State, cell: [i32; 3], block: crate::content::BlockStateId) {
    let [x, y, z] = cell;
    let edits = state.world.prepare_edits(&[(x, y, z, block)]).unwrap();
    let (key, local) = world_to_chunk(x, y, z);
    let version = edits[0].new_version;
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: edits,
        deltas: vec![BlockDelta {
            key,
            version,
            local: local.map(|value| value as u8),
            block,
        }],
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        entity_wakes: Vec::new(),
        entities: None,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None)
            .unwrap()
    );
}

#[test]
fn live_stream_prepares_once_and_shares_encoded_pages_for_matching_clients() {
    let mut fixture = Fixture::new("shared");
    let first = fixture.join(1);
    let second = fixture.join(2);
    durable::publish_committed(&mut fixture.state).unwrap();
    streaming::publish_streams(&mut fixture.state).unwrap();
    let a = snapshots(&first);
    let b = snapshots(&second);
    assert_eq!(a.len(), 2, "chunk start and one player page");
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(&b) {
        let a = a.encode(fixture.state.world.catalog()).unwrap();
        let b = b.encode(fixture.state.world.catalog()).unwrap();
        assert!(
            Arc::ptr_eq(&a, &b),
            "codec reuses the exact encoded allocation"
        );
        crate::protocol::read_server(&a[..]).unwrap();
    }
    assert!(
        matches!(a[1].message(), ServerMessage::EntitySnapshotPage(page) if page.entities.len() == 2)
    );
    let key = fixture.state.clients[&first.id].center;
    assert_eq!(fixture.state.clients[&first.id].sent_epochs[&key], 1);
    assert_eq!(fixture.state.clients[&second.id].sent_epochs[&key], 1);
    assert_eq!(fixture.state.world.pinned_chunk_count(), 1);
}

#[test]
fn differing_epochs_do_not_share_wire_content() {
    let mut fixture = Fixture::new("epochs");
    let first = fixture.join(1);
    let second = fixture.join(2);
    fixture
        .state
        .clients
        .get_mut(&second.id)
        .unwrap()
        .next_snapshot_epoch = 7;
    durable::publish_committed(&mut fixture.state).unwrap();
    streaming::publish_streams(&mut fixture.state).unwrap();
    let a = snapshots(&first);
    let b = snapshots(&second);
    assert!(matches!(a[0].message(), ServerMessage::WorldSnapshotStart(start) if start.epoch == 1));
    assert!(matches!(b[0].message(), ServerMessage::WorldSnapshotStart(start) if start.epoch == 7));
    let a = a[0].encode(fixture.state.world.catalog()).unwrap();
    let b = b[0].encode(fixture.state.world.catalog()).unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
    assert_ne!(a, b);
}

#[test]
fn snapshot_excludes_unreceipted_edit_then_commit_continues_its_revision() {
    let mut fixture = Fixture::new("confirmed");
    let session = fixture.join(1);
    let key = fixture.state.clients[&session.id].center;
    let cell = [key.x * 16 + 5, key.y * 16 + 5, key.z * 16 + 5];
    let before = fixture.state.world.cached_version(key).unwrap();
    let old_block = fixture
        .state
        .world
        .cached_block(cell[0], cell[1], cell[2])
        .unwrap();
    let block = if old_block == AIR { STONE } else { AIR };
    stage_edit(&mut fixture.state, cell, block);
    let (release, held) = mpsc::channel();
    let actual = std::mem::replace(&mut fixture.state.durability.pending[0].receiver, held);
    let receipt = actual.recv_timeout(Duration::from_secs(5)).unwrap();
    durable::poll_journal_receipts(&mut fixture.state).unwrap();
    durable::publish_committed(&mut fixture.state).unwrap();
    streaming::publish_streams(&mut fixture.state).unwrap();
    let initial = snapshots(&session);
    let (_, local) = world_to_chunk(cell[0], cell[1], cell[2]);
    assert!(
        matches!(initial[0].message(), ServerMessage::WorldSnapshotStart(start)
        if start.chunk.version == before && start.chunk.block(local) == Some(old_block))
    );
    release.send(receipt).unwrap();
    durable::complete_barrier(&mut fixture.state, CommitBarrier::AllStaged).unwrap();
    durable::publish_committed(&mut fixture.state).unwrap();
    let changes: Vec<_> = session
        .receiver
        .try_iter()
        .map(OutboundFrame::into_message)
        .collect();
    assert!(
        matches!(&changes[..], [ServerMessage::WorldCommitPart(part)]
        if part.key == key && part.epoch == 1 && part.block_from == before
        && part.block_to > before && part.blocks[0].block == block)
    );
}

#[test]
fn captured_revision_is_rejected_after_confirmed_edit_and_recaptured_in_order() {
    let mut fixture = Fixture::new("stale-edit");
    let session = fixture.join(1);
    let key = fixture.state.clients[&session.id].center;
    durable::publish_committed(&mut fixture.state).unwrap();
    let selected = selection(&fixture.state, &[&session], key);
    let batch = dispatch(&mut fixture.state, selected).unwrap();
    let cell = [key.x * 16 + 5, key.y * 16 + 5, key.z * 16 + 5];
    let old = fixture
        .state
        .world
        .cached_block(cell[0], cell[1], cell[2])
        .unwrap();
    stage_edit(
        &mut fixture.state,
        cell,
        if old == AIR { STONE } else { AIR },
    );
    durable::complete_barrier(&mut fixture.state, CommitBarrier::AllStaged).unwrap();
    durable::publish_committed(&mut fixture.state).unwrap();
    finish(&mut fixture.state, batch).unwrap();
    assert!(snapshots(&session).is_empty());
    assert!(!fixture.state.clients[&session.id].sent.contains(&key));
    streaming::publish_streams(&mut fixture.state).unwrap();
    let fresh = snapshots(&session);
    let version = fixture.state.world.cached_version(key).unwrap();
    assert!(
        matches!(fresh[0].message(), ServerMessage::WorldSnapshotStart(start) if start.chunk.version == version && start.epoch == 1)
    );
    crate::server::handle_message(
        &mut fixture.state,
        session.id,
        ClientMessage::Resync { key },
    )
    .unwrap();
    streaming::publish_streams(&mut fixture.state).unwrap();
    let resync = snapshots(&session);
    assert!(
        matches!(resync[0].message(), ServerMessage::WorldSnapshotStart(start) if start.epoch == 2 && start.chunk.version == version)
    );
}

#[test]
fn changed_interest_or_reconnected_session_cannot_receive_pending_capture() {
    let mut fixture = Fixture::new("session");
    let first = fixture.join(1);
    let second = fixture.join(2);
    durable::publish_committed(&mut fixture.state).unwrap();
    let key = fixture.state.clients[&first.id].center;
    let selected = selection(&fixture.state, &[&first, &second], key);
    let batch = dispatch(&mut fixture.state, selected).unwrap();
    fixture.state.clients.get_mut(&first.id).unwrap().radius += 1;
    // Do not change the entity revision until the first result is rejected on
    // its interest identity, rather than incidentally on a despawn revision.
    finish(&mut fixture.state, batch).unwrap();
    assert!(snapshots(&first).is_empty());
    assert!(!snapshots(&second).is_empty());
    let selected = selection(&fixture.state, &[&first], key);
    let batch = dispatch(&mut fixture.state, selected).unwrap();
    fixture.state.remove_client(first.id);
    let replacement = fixture.join(1);
    assert_ne!(replacement.id, first.id);
    finish(&mut fixture.state, batch).unwrap();
    assert!(snapshots(&replacement).is_empty());
    durable::publish_committed(&mut fixture.state).unwrap();
    streaming::publish_streams(&mut fixture.state).unwrap();
    assert!(!snapshots(&replacement).is_empty());
}

#[test]
fn slow_snapshot_reader_defers_without_losing_eligibility_or_retaining_jobs() {
    let mut fixture = Fixture::new("backpressure");
    let slow = fixture.join(1);
    let healthy = fixture.join(2);
    durable::publish_committed(&mut fixture.state).unwrap();
    for nonce in 0..(OUTBOUND_FRAME_CAPACITY - super::super::SNAPSHOT_FRAME_HEADROOM) {
        fixture.state.clients[&slow.id]
            .sender
            .try_send(ServerMessage::Pong {
                nonce: nonce as u64,
            })
            .unwrap();
    }
    streaming::publish_streams(&mut fixture.state).unwrap();
    assert!(fixture.state.clients[&slow.id].sent.is_empty());
    assert!(!snapshots(&healthy).is_empty());
    assert!(
        snapshots(&slow).is_empty(),
        "drain slow queue without a partial snapshot"
    );
    streaming::publish_streams(&mut fixture.state).unwrap();
    assert!(
        !snapshots(&slow).is_empty(),
        "unsent snapshot retries after queue drains"
    );
    assert!(fixture.state.clients.contains_key(&slow.id));
}

#[test]
fn distinct_chunk_capture_is_bounded_and_rotates_to_deferred_clients() {
    let mut fixture = Fixture::new("bounded");
    let mut sessions = Vec::new();
    for index in 0..MAX_SNAPSHOT_JOBS + 1 {
        let session = fixture.join(index as u128 + 1);
        let key = ChunkKey {
            x: index as i32 * 20,
            y: 8,
            z: 0,
        };
        fixture.state.world.get_chunk(key).unwrap();
        fixture.state.clients.get_mut(&session.id).unwrap().center = key;
        sessions.push(session);
    }
    durable::publish_committed(&mut fixture.state).unwrap();
    streaming::publish_streams(&mut fixture.state).unwrap();
    let sent = sessions
        .iter()
        .filter(|session| !fixture.state.clients[&session.id].sent.is_empty())
        .count();
    assert_eq!(sent, MAX_SNAPSHOT_JOBS);
    for session in &sessions {
        snapshots(session);
    }
    streaming::publish_streams(&mut fixture.state).unwrap();
    assert!(
        sessions
            .iter()
            .all(|session| !fixture.state.clients[&session.id].sent.is_empty())
    );
}

#[test]
fn snapshots_do_not_overtake_player_removals_queued_by_slow_client_disconnects() {
    let mut fixture = Fixture::new("disconnect-order");
    let slow_a = fixture.join(1);
    let slow_b = fixture.join(2);
    let healthy = fixture.join(3);
    durable::publish_committed(&mut fixture.state).unwrap();
    for session in [&slow_a, &slow_b] {
        let (sender, receiver) = fixture.state.outbound.client_queue();
        drop(receiver);
        let client = fixture.state.clients.get_mut(&session.id).unwrap();
        client.sender = sender;
        // A previously visible drop is now absent. Streaming must send the
        // empty replacement, discover the closed queue, and remove the player.
        client.last_sent_drops.push(crate::protocol::DroppedItem {
            id: 1,
            item: crate::items::ItemId::new(STONE.get()),
            count: 1,
            position: [0.5, 80.0, 0.5],
            age_ms: 0,
        });
    }
    streaming::publish_streams(&mut fixture.state).unwrap();
    assert_eq!(fixture.state.clients.len(), 1);
    assert_eq!(fixture.state.durability.publish_queue.len(), 2);
    assert!(snapshots(&healthy).is_empty());
    durable::publish_committed(&mut fixture.state).unwrap();
    streaming::publish_streams(&mut fixture.state).unwrap();
    let frames = snapshots(&healthy);
    assert!(
        matches!(frames[1].message(), ServerMessage::EntitySnapshotPage(page) if page.entities.len() == 1)
    );
    assert_eq!(fixture.state.clients[&healthy.id].next_snapshot_epoch, 2);
    // The next committed-effects pass cannot regress a fresh snapshot to an
    // older despawn revision. All those effects preceded this initial epoch.
    durable::publish_committed(&mut fixture.state).unwrap();
}

#[test]
fn dense_snapshot_disconnects_only_affected_client_and_closes_earlier_jobs() {
    use crate::server::{
        drops,
        entities::{EntityPayload, EntitySpawn},
    };
    let mut fixture = Fixture::new("dense-local");
    let healthy = fixture.join(1);
    let dense = fixture.join(2);
    let key = ChunkKey { x: 20, y: 8, z: 0 };
    fixture.state.world.get_chunk(key).unwrap();
    fixture.state.clients.get_mut(&dense.id).unwrap().center = key;
    let payload = drops::DropEntityPayload::new(
        crate::inventory::Stack {
            item: crate::items::ItemId::new(STONE.get()),
            count: 1,
            components: None,
        },
        drops::unix_ms(),
        Duration::ZERO,
    );
    let entities = fixture
        .state
        .entities
        .prepare_spawn_batch(
            (0..1025)
                .map(|_| EntitySpawn::Mobile {
                    entity_type: drops::DROP_ENTITY_TYPE,
                    position: [320.5, 128.5, 0.5],
                    payload: EntityPayload::new(payload.clone()),
                    spawn_tick: 1,
                })
                .collect(),
        )
        .unwrap();
    let permit = fixture
        .state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        entity_wakes: Vec::new(),
        entities: Some(entities),
    };
    assert!(
        fixture
            .state
            .durability
            .try_stage(TickId::new(1), &action, Some(permit))
            .unwrap()
    );
    durable::complete_barrier(&mut fixture.state, CommitBarrier::AllStaged).unwrap();
    durable::publish_committed(&mut fixture.state).unwrap();
    let healthy_key = fixture.state.clients[&healthy.id].center;
    assert!(
        healthy_key < key,
        "healthy group is accepted before oversized capture"
    );
    let mut selected = selection(&fixture.state, &[&healthy], healthy_key);
    selected.select(dense.id, &fixture.state.clients[&dense.id], key);
    let batch = dispatch(&mut fixture.state, selected).unwrap();
    finish(&mut fixture.state, batch).unwrap();
    assert!(fixture.state.clients.contains_key(&healthy.id));
    assert!(!fixture.state.clients.contains_key(&dense.id));
    assert!(!snapshots(&healthy).is_empty());
    assert!(snapshots(&dense).is_empty());
    durable::publish_committed(&mut fixture.state).unwrap();
    crate::server::handle_message(
        &mut fixture.state,
        healthy.id,
        ClientMessage::Resync { key: healthy_key },
    )
    .unwrap();
    streaming::publish_streams(&mut fixture.state).unwrap();
    assert!(
        !snapshots(&healthy).is_empty(),
        "next batch remains usable after capacity failure"
    );
}

#[test]
fn worker_interest_change_releases_old_pins_before_publishing_new_center() {
    let mut fixture = Fixture::new("interest-worker");
    let session = fixture.join(1);
    durable::publish_committed(&mut fixture.state).unwrap();
    streaming::publish_streams(&mut fixture.state).unwrap();
    snapshots(&session);
    let old = fixture.state.clients[&session.id].center;
    let new = ChunkKey {
        x: old.x + 20,
        ..old
    };
    fixture.state.world.get_chunk(new).unwrap();
    fixture.state.clients.get_mut(&session.id).unwrap().center = new;
    streaming::publish_streams(&mut fixture.state).unwrap();
    assert!(!fixture.state.clients[&session.id].sent.contains(&old));
    assert!(fixture.state.clients[&session.id].sent.contains(&new));
    assert_eq!(fixture.state.world.pinned_chunk_count(), 1);
    assert!(snapshots(&session).iter().any(|frame| matches!(frame.message(), ServerMessage::WorldSnapshotStart(start) if start.chunk.key == new)));
}

#[test]
fn pressure_reclaim_is_worker_prepared_and_coordinator_announces_then_releases() {
    let mut fixture = Fixture::new("pressure-worker");
    let session = fixture.join(1);
    let center = fixture.state.clients[&session.id].center;
    let radius = fixture.state.clients[&session.id].radius;
    let edge = ChunkKey {
        x: center.x + i32::from(radius),
        ..center
    };
    fixture.state.world.reset_cache_for_test(2);
    for key in [center, edge] {
        let chunk = fixture.state.world.get_chunk(key).unwrap();
        assert!(fixture.state.world.pin_resident_chunk(key));
        let client = fixture.state.clients.get_mut(&session.id).unwrap();
        client.sent.insert(key);
        client.sent_epochs.insert(key, 1);
        client.sent_block_versions.insert(key, chunk.version);
        client
            .sent_entity_revisions
            .insert(key, fixture.state.entity_public_revision);
    }
    assert!(!fixture.state.world.can_admit_chunk());
    super::super::interest_projection::reduce_view_under_pressure(&mut fixture.state).unwrap();
    assert!(
        matches!(session.receiver.try_recv().unwrap().message(), ServerMessage::ViewDistance { radius: reduced } if *reduced == radius - 1)
    );
    assert_eq!(fixture.state.clients[&session.id].sent.len(), 1);
    assert!(fixture.state.clients[&session.id].sent.contains(&center));
    assert!(fixture.state.world.can_admit_chunk());
    assert_eq!(fixture.state.world.pinned_chunk_count(), 1);
}

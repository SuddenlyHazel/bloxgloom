use super::*;
use crate::server::streaming::workers::Workers;

struct Fixture {
    state: State,
    _save: Save,
}
struct Save(std::path::PathBuf);
impl Drop for Save {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Fixture {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-projection-{}-{stamp}",
            std::process::id()
        ));
        Self {
            state: crate::server::server_state(7, path.clone()).unwrap(),
            _save: Save(path),
        }
    }
    fn join(
        &mut self,
        profile: u128,
    ) -> (
        u64,
        std::sync::mpsc::Receiver<crate::server::outbound::OutboundFrame>,
        std::net::TcpStream,
    ) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        let (sender, receiver) = self.state.outbound.client_queue();
        let id = crate::server::join_client(
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
        (id, receiver, peer)
    }
}

fn effect() -> PublishEffects {
    PublishEffects {
        spawned: Default::default(),
        client_id: None,
        profile: None,
        action_id: None,
        accepted: true,
        reason: String::new(),
        inventory: None,
        deltas: Vec::new(),
        entity_commit: None,
        pickups: Vec::new(),
        sounds: Vec::new(),
        fire_bursts: Vec::new(),
    }
}

#[test]
fn initial_snapshot_then_ordered_worker_commits_keep_pickup_events_after_inventory() {
    use crate::server::outbound::OutboundFrame;
    let mut fixture = Fixture::new();
    let (id, receiver, _peer) = fixture.join(1);
    publish(&mut fixture.state).unwrap();
    crate::server::streaming::publish_streams(&mut fixture.state).unwrap();
    let key = fixture.state.clients[&id].center;
    assert!(
        receiver
            .try_iter()
            .any(|frame| matches!(frame.message(), ServerMessage::WorldSnapshotStart(_)))
    );
    receiver.try_iter().for_each(drop);
    let from = fixture.state.clients[&id].sent_block_versions[&key];
    for offset in 1..=2 {
        let mut effect = effect();
        effect.deltas.push(crate::server::durable::BlockDelta {
            key,
            version: from + offset,
            local: [5, 5, 5],
            block: crate::world::STONE,
        });
        if offset == 2 {
            effect.client_id = Some(id);
            effect.profile = Some(1);
            effect.action_id = Some(1);
            effect.inventory = Some(Default::default());
            effect.pickups.push(crate::protocol::DroppedItem {
                id: 9,
                item: crate::items::ItemId::new(crate::world::STONE.get()),
                count: 1,
                components: None,
                position: [0.5, 80.0, 0.5],
                age_ms: 20,
            });
        }
        fixture.state.durability.publish_queue.push(effect);
    }
    publish(&mut fixture.state).unwrap();
    let messages: Vec<_> = receiver
        .try_iter()
        .map(OutboundFrame::into_message)
        .collect();
    assert!(
        matches!(&messages[..], [ServerMessage::WorldCommitPart(a), ServerMessage::WorldCommitPart(b), ServerMessage::ActionResult {..}, ServerMessage::Inventory {..}, ServerMessage::Pickups {..}]
        if a.block_from == from && a.block_to == from + 1 && b.block_from == a.block_to && b.block_to == from + 2 && a.commit_id < b.commit_id && a.epoch == b.epoch)
    );
}

#[test]
fn component_pickup_fanout_pages_all_stacks_after_inventory_with_exact_payloads() {
    let mut fixture = Fixture::new();
    let (id, receiver, _peer) = fixture.join(1);
    let mut effect = effect();
    effect.client_id = Some(id);
    effect.profile = Some(1);
    effect.inventory = Some(Default::default());
    effect.pickups = (1..=256)
        .map(|id| crate::protocol::DroppedItem {
            id,
            item: crate::items::STICK,
            count: 128,
            components: crate::inventory::Stack::with_components(
                crate::items::STICK,
                128,
                1,
                vec![id as u8; 1024],
            )
            .unwrap()
            .components,
            position: [0.; 3],
            age_ms: 1000,
        })
        .collect();
    let prepared = prepare(
        Capture::new(id, &fixture.state.clients[&id]),
        &effect,
        None,
        None,
        &SharedParts::default(),
    )
    .unwrap();
    apply(&mut fixture.state, prepared);
    let frames: Vec<_> = receiver.try_iter().collect();
    assert!(matches!(
        frames[0].message(),
        ServerMessage::Inventory { .. }
    ));
    assert_eq!(frames.len(), 6);
    let mut received = Vec::new();
    for frame in &frames[1..] {
        let mut bytes = Vec::new();
        crate::protocol::write_server(&mut bytes, frame.message()).unwrap();
        assert!(bytes.len() <= crate::protocol::MAX_FRAME + 4);
        if let ServerMessage::Pickups { items } =
            crate::protocol::read_server(bytes.as_slice()).unwrap()
        {
            received.extend(items);
        } else {
            panic!("unexpected frame after inventory");
        }
    }
    assert_eq!(received, effect.pickups);
}

#[test]
fn stale_subscription_output_closes_session_without_advancing_revisions() {
    let mut fixture = Fixture::new();
    let (id, receiver, _peer) = fixture.join(1);
    publish(&mut fixture.state).unwrap();
    crate::server::streaming::publish_streams(&mut fixture.state).unwrap();
    receiver.try_iter().for_each(drop);
    let key = fixture.state.clients[&id].center;
    let capture = Capture::new(id, &fixture.state.clients[&id]);
    let pending = prepare(capture, &effect(), None, None, &SharedParts::default()).unwrap();
    crate::server::handle_message(
        &mut fixture.state,
        id,
        crate::protocol::ClientMessage::Resync { key },
    )
    .unwrap();
    apply(&mut fixture.state, pending);
    assert!(!fixture.state.clients.contains_key(&id));
    assert!(receiver.try_recv().is_err());
    assert_eq!(fixture.state.world.pinned_chunk_count(), 0);
}

#[test]
fn oversized_fanout_resyncs_whole_epochs_and_a_slow_peer_does_not_block_others() {
    let mut fixture = Fixture::new();
    let (slow, slow_receiver, _slow_peer) = fixture.join(1);
    let (healthy, receiver, _peer) = fixture.join(2);
    publish(&mut fixture.state).unwrap();
    crate::server::streaming::publish_streams(&mut fixture.state).unwrap();
    slow_receiver.try_iter().for_each(drop);
    receiver.try_iter().for_each(drop);
    let key = fixture.state.clients[&healthy].center;
    let mut oversized = effect();
    oversized.deltas = vec![
        crate::server::durable::BlockDelta {
            key,
            version: 100,
            local: [0, 0, 0],
            block: crate::world::STONE
        };
        4097
    ];
    fixture.state.durability.publish_queue.push(oversized);
    publish(&mut fixture.state).unwrap();
    assert!(fixture.state.clients[&healthy].sent.is_empty());
    assert!(receiver.try_recv().is_err());
    crate::server::streaming::publish_streams(&mut fixture.state).unwrap();
    assert!(receiver.try_iter().any(|frame| matches!(frame.message(), ServerMessage::WorldSnapshotStart(start) if start.epoch == 2)));
    receiver.try_iter().for_each(drop);
    let (sender, held) = fixture
        .state
        .outbound
        .client_queue_with_limits(1, 2 * 1024 * 1024);
    sender.try_send(ServerMessage::Pong { nonce: 1 }).unwrap();
    fixture.state.clients.get_mut(&slow).unwrap().sender = sender;
    let mut update = effect();
    update.deltas.push(crate::server::durable::BlockDelta {
        key,
        version: 101,
        local: [0, 0, 0],
        block: crate::world::STONE,
    });
    fixture.state.durability.publish_queue.push(update);
    publish(&mut fixture.state).unwrap();
    assert!(!fixture.state.clients.contains_key(&slow));
    assert!(fixture.state.clients.contains_key(&healthy));
    assert!(
        receiver
            .try_iter()
            .any(|frame| matches!(frame.message(), ServerMessage::WorldCommitPart(_)))
    );
    drop(held);
    publish(&mut fixture.state).unwrap(); // ordered player despawn after previous effect
}

#[test]
fn worker_fanout_shares_identical_parts_but_not_different_subscription_epochs() {
    let mut fixture = Fixture::new();
    let (first, a, _a) = fixture.join(1);
    let (_, b, _b) = fixture.join(2);
    let (third, c, _c) = fixture.join(3);
    publish(&mut fixture.state).unwrap();
    crate::server::streaming::publish_streams(&mut fixture.state).unwrap();
    a.try_iter().for_each(drop);
    b.try_iter().for_each(drop);
    c.try_iter().for_each(drop);
    let key = fixture.state.clients[&first].center;
    fixture
        .state
        .clients
        .get_mut(&third)
        .unwrap()
        .sent_epochs
        .insert(key, 7);
    let mut update = effect();
    update.deltas.push(crate::server::durable::BlockDelta {
        key,
        version: fixture.state.clients[&first].sent_block_versions[&key] + 1,
        local: [0, 0, 0],
        block: crate::world::STONE,
    });
    fixture.state.durability.publish_queue.push(update);
    publish(&mut fixture.state).unwrap();
    let a = a.try_recv().unwrap();
    let b = b.try_recv().unwrap();
    let c = c.try_recv().unwrap();
    assert!(matches!(c.message(), ServerMessage::WorldCommitPart(part) if part.epoch == 7));
    let catalog = fixture.state.world.catalog();
    let a = a.encode(catalog).unwrap();
    let b = b.encode(catalog).unwrap();
    let c = c.encode(catalog).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert!(!Arc::ptr_eq(&a, &c));
    assert_ne!(a, c);
}

#[test]
fn publication_workers_restore_order_and_close_panicking_batches() {
    let mut workers = Workers::new(2).unwrap();
    let coordinator = std::thread::current().id();
    let (release, wait) = std::sync::mpsc::sync_channel(1);
    let results = workers
        .run(vec![
            Box::new(move || {
                assert_ne!(coordinator, std::thread::current().id());
                wait.recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                1
            }),
            Box::new(move || {
                release.send(()).unwrap();
                2
            }),
        ])
        .unwrap();
    assert_eq!(results, vec![Ok(1), Ok(2)]);
    assert_eq!(
        workers
            .run(vec![
                Box::new(|| panic!("projection probe")),
                Box::new(|| 3)
            ])
            .unwrap(),
        vec![Err(()), Ok(3)]
    );
    assert_eq!(workers.run(vec![Box::new(|| 4)]).unwrap(), vec![Ok(4)]);
}

#[test]
fn fire_cue_requires_subscription_and_follows_committed_world_frame() {
    use crate::server::outbound::OutboundFrame;
    let mut fixture = Fixture::new();
    let (id, receiver, _peer) = fixture.join(1);
    publish(&mut fixture.state).unwrap();
    crate::server::streaming::publish_streams(&mut fixture.state).unwrap();
    receiver.try_iter().for_each(drop);
    let key = fixture.state.clients[&id].center;
    let from = fixture.state.clients[&id].sent_block_versions[&key];
    let mut change = effect();
    change.deltas.push(crate::server::durable::BlockDelta {
        key,
        version: from + 1,
        local: [5, 5, 5],
        block: crate::world::AIR,
    });
    let world_cell = [key.x * 16 + 5, key.y * 16 + 5, key.z * 16 + 5];
    change.fire_bursts = vec![world_cell, [10_000, 80, 10_000]];
    fixture.state.durability.publish_queue.push(change);
    publish(&mut fixture.state).unwrap();
    let messages: Vec<_> = receiver
        .try_iter()
        .map(OutboundFrame::into_message)
        .collect();
    assert!(
        matches!(&messages[..], [ServerMessage::WorldCommitPart(_), ServerMessage::FireBursts { cells }] if cells == &[world_cell])
    );
}

#[test]
fn fire_cue_is_dropped_for_backlogged_client_without_disconnect() {
    let mut fixture = Fixture::new();
    let (id, receiver, _peer) = fixture.join(1);
    publish(&mut fixture.state).unwrap();
    crate::server::streaming::publish_streams(&mut fixture.state).unwrap();
    receiver.try_iter().for_each(drop);
    let key = fixture.state.clients[&id].center;
    for nonce in 0..OUTBOUND_FRAME_CAPACITY / 2 {
        fixture.state.clients[&id]
            .sender
            .try_send(ServerMessage::Pong {
                nonce: nonce as u64,
            })
            .unwrap();
    }
    let mut change = effect();
    change
        .fire_bursts
        .push([key.x * 16 + 5, key.y * 16 + 5, key.z * 16 + 5]);
    fixture.state.durability.publish_queue.push(change);
    publish(&mut fixture.state).unwrap();
    assert!(fixture.state.clients.contains_key(&id));
    assert!(
        receiver
            .try_iter()
            .all(|frame| !matches!(frame.message(), ServerMessage::FireBursts { .. }))
    );
}

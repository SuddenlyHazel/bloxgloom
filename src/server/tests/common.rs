//! Shared fixtures and coordinator-driving helpers for server integration tests.

use super::super::outbound::OutboundFrame;
use super::super::*;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) use super::super::durable::DurableRequest;

static TEST_SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct TestSave(PathBuf);

impl TestSave {
    pub(super) fn new(label: &str) -> Self {
        let sequence = TEST_SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-server-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestSave {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) struct Session {
    pub(super) id: u64,
    pub(super) action_epoch: u64,
    pub(super) joined: JoinedSnapshot,
    pub(super) receiver: Receiver<OutboundFrame>,
    _peer: TcpStream,
}

pub(super) struct JoinedSnapshot {
    pub(super) position: [f32; 3],
    pub(super) inventory: Inventory,
}

pub(super) fn state_for(save: &TestSave, seed: u64) -> State {
    server_state(seed, save.path().to_path_buf()).unwrap()
}

pub(super) fn save_inventory(save: &TestSave, profile: u128, inventory: &Inventory) {
    let store = InventoryStore::new(save.path()).unwrap();
    store
        .checkpoint_snapshot(
            profile,
            &InventoryStore::encode_snapshot(inventory).unwrap(),
        )
        .unwrap();
}

pub(super) fn run_tick(state: &mut State, tick: &mut u64, ready: Vec<SimulationInput>) {
    let current = *tick;
    *tick += 1;
    tick_with_inputs(
        state,
        TickId::new(current),
        Instant::now(),
        Vec::new(),
        ready,
    )
    .unwrap();
}

pub(super) fn run_empty_tick(state: &mut State, tick: &mut u64) {
    run_tick(state, tick, Vec::new());
}

/// Join through the same coordinator message used by the network layer.
pub(super) fn join(state: &mut State, tick: &mut u64, profile: u128) -> Session {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    // These in-process sessions deliberately do not run socket readers while
    // other joins advance many ticks. Give the fixture room for their ordered
    // terrain frames; slow-peer tests install a deliberately tiny queue.
    let (sender, receiver) = state
        .outbound
        .client_queue_with_limits(1024, 8 * 1024 * 1024);
    let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
    let inventory = state.inventory_store.load(profile).unwrap();
    run_tick(
        state,
        tick,
        vec![SimulationInput::Join {
            profile,
            inventory: Box::new(inventory),
            sender,
            socket,
            reply: reply_sender,
        }],
    );

    for _ in 0..500 {
        match reply_receiver.try_recv() {
            Ok(JoinResponse::Completed(result)) => match *result {
                Ok(joined) => {
                    let client = state.clients.get(&joined.id).unwrap();
                    return Session {
                        id: joined.id,
                        action_epoch: state.durability.receipt_ledger(profile).current_epoch(),
                        joined: JoinedSnapshot {
                            position: client.position(),
                            inventory: client.inventory.clone(),
                        },
                        receiver,
                        _peer: peer,
                    };
                }
                Err(error) => panic!("coordinator rejected test join: {error}"),
            },
            Ok(JoinResponse::RefreshInventory) => {
                panic!("test join unexpectedly used a stale inventory snapshot")
            }
            Err(mpsc::TryRecvError::Disconnected) => panic!("join reply channel closed"),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        run_empty_tick(state, tick);
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("coordinator did not finish the test join");
}

impl Session {
    pub(super) fn action_id(&self, sequence: u64) -> u128 {
        (u128::from(self.action_epoch) << 64) | u128::from(sequence)
    }
}

pub(super) fn messages(session: &Session) -> Vec<ServerMessage> {
    session
        .receiver
        .try_iter()
        .map(OutboundFrame::into_message)
        .collect()
}

pub(super) fn command_and_wait(
    state: &mut State,
    tick: &mut u64,
    session: &Session,
    sequence: u64,
    action_id: u128,
    message: ClientMessage,
) -> Vec<ServerMessage> {
    run_tick(
        state,
        tick,
        vec![SimulationInput::Command {
            id: session.id,
            sequence,
            message,
        }],
    );

    let mut output = Vec::new();
    for _ in 0..1_000 {
        while let Ok(frame) = session.receiver.try_recv() {
            let message = frame.into_message();
            let finished = matches!(
                &message,
                ServerMessage::ActionResult { action_id: received, .. }
                    if *received == action_id
            );
            output.push(message);
            if finished {
                return output;
            }
        }
        run_empty_tick(state, tick);
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("action {action_id} did not receive a coordinator result");
}

pub(super) fn wait_for_inventory(
    state: &mut State,
    tick: &mut u64,
    id: u64,
    predicate: impl Fn(&Inventory) -> bool,
) {
    for _ in 0..1_000 {
        if state
            .clients
            .get(&id)
            .is_some_and(|client| predicate(&client.inventory))
        {
            return;
        }
        run_empty_tick(state, tick);
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("inventory condition was not reached");
}

pub(super) fn action_result(messages: &[ServerMessage], action_id: u128) -> Option<bool> {
    messages.iter().find_map(|message| match message {
        ServerMessage::ActionResult {
            action_id: received,
            accepted,
            ..
        } if *received == action_id => Some(*accepted),
        _ => None,
    })
}

/// Stages one drop spawn through the WAL and drains its receipt, so the
/// live store and the checkpoint mirror advance together exactly as they do
/// for gameplay spawns.
pub(super) fn spawn_drop(
    state: &mut State,
    tick: u64,
    position: [f32; 3],
    item: crate::items::ItemId,
    count: u16,
    delay: Duration,
) {
    let catalog = state.world.catalog_arc();
    let batch = crate::server::drops::plan_spawns(
        &state.entities,
        &catalog,
        &[(position, item, count, delay)],
        tick,
        crate::server::drops::unix_ms(),
    )
    .unwrap()
    .expect("test drop spawn plans work");
    stage_entity_batch(state, tick, batch);
}

/// Stages one drop take through the WAL and drains its receipt.
pub(super) fn take_drop(
    state: &mut State,
    tick: u64,
    id: crate::server::entities::EntityId,
    count: u16,
) {
    let batch = crate::server::drops::plan_take(&state.entities, &[(id, count)])
        .unwrap()
        .expect("test drop take plans work");
    stage_entity_batch(state, tick, batch);
}

fn stage_entity_batch(
    state: &mut State,
    tick: u64,
    batch: crate::server::entities::PreparedEntityBatch,
) {
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the test drop batch");
    let action = crate::server::durable::CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entity_wakes: Vec::new(),
        entities: Some(batch),
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(tick), &action, Some(permit))
            .unwrap()
    );
    drain_durable(state, tick);
}

/// Polls WAL receipts until every staged transaction has applied. Trajectory
/// comparisons drain after each tick so both states apply the same staged
/// steps before their snapshots are compared: without this, receipt timing
/// (wall-clock) would decide how many staged steps are visible at a tick
/// index, which is a test-scheduling artifact rather than physics.
pub(super) fn drain_durable(state: &mut State, tick: u64) {
    for _ in 0..2_000 {
        crate::server::durable::process_durable_actions(state, TickId::new(tick), Instant::now())
            .unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        state.durability.pending.is_empty(),
        "test drop batch must commit"
    );
}

pub(super) fn drop_nearby(state: &State, position: [f32; 3]) -> Vec<crate::protocol::DroppedItem> {
    crate::server::drops::nearby(&state.entities, position)
}

pub(super) fn drop_candidates(
    state: &State,
    position: [f32; 3],
) -> Vec<crate::protocol::DroppedItem> {
    crate::server::drops::pickup_candidates(&state.entities, position)
}

pub(super) fn drop_stack(
    state: &State,
    id: crate::server::entities::EntityId,
) -> Option<crate::inventory::Stack> {
    crate::server::drops::stack(&state.entities, id)
}

pub(super) fn drop_active_len(state: &State) -> usize {
    crate::server::drops::airborne_count(&state.entities)
}

/// Forces the 27-chunk neighbourhood around a position resident through
/// synchronous generation. Determinism pins use this to take the async
/// chunk loader out of the equation; loader convergence itself is covered
/// by the settle pins, which sleep for delivery.
pub(super) fn reside_neighbourhood(state: &mut State, center: [f32; 3]) {
    let (base, _) = crate::world::world_to_chunk(
        center[0].floor() as i32,
        center[1].floor() as i32,
        center[2].floor() as i32,
    );
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                let _ = state.world.get_block(
                    (base.x + dx) * 16 + 8,
                    (base.y + dy) * 16 + 8,
                    (base.z + dz) * 16 + 8,
                );
            }
        }
    }
}

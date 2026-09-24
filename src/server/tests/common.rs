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

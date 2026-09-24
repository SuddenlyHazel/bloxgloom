//! Authoritative TCP server. The simulation coordinator owns mutable world,
//! inventory, and drop state; chunk loading and journal fsync run on bounded
//! workers. Durable actions become visible only after their WAL receipt.
mod builtins;
mod checkpoint;
mod chunk_loader;
mod drops;
mod durable;
mod effects;
mod interest;
mod journal;
mod loot;
mod metrics;
mod movement;
mod net;
mod outbound;
mod parallel;
mod perf;
mod registry;
mod runtime;
mod simulation;
mod spawn;
mod streaming;
mod voxel_view;

pub use perf::run_perf_benchmark;

use crate::inventory::{Inventory, InventoryStore};
#[cfg(test)]
use crate::protocol;
use crate::protocol::{
    ClientMessage, DroppedItem, MAX_VIEW_DISTANCE, MIN_VIEW_DISTANCE, ServerMessage,
};
use crate::world::{AIR, BEDROCK_Y, ChunkKey, World, world_to_chunk};
use builtins::builtin_phase_plan;
use chunk_loader::ChunkLoader;
use drops::Drops;
use durable::{
    Durability, handle_live_message, process_durable_actions, publish_committed,
    queue_interaction_actions, remember_drops_checkpoint,
};
use effects::CellCoord;
use metrics::MetricsRecorder;
use movement::{MovementBatch, MovementCommand, MovementState};
use net::serve_listener;
use outbound::{OutboundFrame, OutboundTelemetry};
use parallel::PhaseExecutor;
use registry::PhasePlan;
use runtime::run_simulation_ticks;
#[cfg(test)]
use runtime::tick_once;
#[cfg(test)]
use runtime::tick_with_inputs;
use simulation::{FIXED_STEP, Phase, TickId};
#[cfg(test)]
use spawn::collides;
use spawn::{spawn_position, spawn_position_cached};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, ErrorKind};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(test)]
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

const MAX_CLIENTS: usize = 16;
const OUTBOUND_CAPACITY: usize = 128;
const DEFAULT_VIEW: u8 = 3;
const STREAM_INTERVAL: Duration = FIXED_STEP;
const INPUT_CAPACITY: usize = 1024;
const MAX_CATCH_UP_TICKS: usize = 3;
const EDIT_REACH: f32 = 8.0;
const SERVER_CHUNK_CACHE: usize = 16_384;
const LOADER_CAPACITY: usize = 256;
const MOVEMENT_QUEUE_CAPACITY: usize = 256;

struct Client {
    profile: u128,
    inventory: Inventory,
    last_drops_revision: u64,
    last_drop_anchor: [i32; 3],
    last_sent_drops: Vec<DroppedItem>,
    sender: SyncSender<OutboundFrame>,
    outbound: Arc<OutboundTelemetry>,
    socket: TcpStream,
    sent: HashSet<ChunkKey>,
    center: ChunkKey,
    radius: u8,
    movement: MovementState,
    pending_moves: VecDeque<MovementCommand>,
}

impl Client {
    fn position(&self) -> [f32; 3] {
        self.movement.position()
    }

    fn enqueue(&self, message: ServerMessage) -> bool {
        if self.outbound.try_send(&self.sender, message) {
            true
        } else {
            eprintln!("disconnecting client: outbound queue full or receiver closed");
            let _ = self.socket.shutdown(Shutdown::Both);
            false
        }
    }

    fn interested(&self, key: ChunkKey) -> bool {
        (key.x as i64 - self.center.x as i64).abs() <= self.radius as i64
            && (key.y as i64 - self.center.y as i64).abs() <= 1
            && (key.z as i64 - self.center.z as i64).abs() <= self.radius as i64
    }
}

struct State {
    world: World,
    inventory_store: InventoryStore,
    drops: Drops,
    seed: u64,
    clients: HashMap<u64, Client>,
    next_id: u64,
    last_drop_save: Instant,
    moving_drops_dirty: bool,
    drops_landed_dirty: bool,
    pending_block_changes: Vec<CellCoord>,
    durability: Durability,
    phase_plan: PhasePlan,
    loader: ChunkLoader,
    movement_executor: PhaseExecutor<MovementBatch, ()>,
    metrics: MetricsRecorder,
    stream_cursor: u64,
    spawn_anchor: [f32; 3],
    pending_joins: VecDeque<PendingJoin>,
    tick_backlog: u64,
    outbound: Arc<OutboundTelemetry>,
    last_sent_bytes: u64,
    last_rejections: u64,
}

struct PendingJoin {
    profile: u128,
    inventory: Inventory,
    sender: SyncSender<OutboundFrame>,
    socket: TcpStream,
    reply: SyncSender<JoinResponse>,
    queued_at: Instant,
}

struct JoinReply {
    id: u64,
    seed: u64,
    position: [f32; 3],
    inventory: Inventory,
}

/// A socket thread may have read its BGIN snapshot before a newer WAL action
/// checkpointed. Refresh is explicit so a delayed join never reuses that
/// captured inventory after the coordinator's overlay has been released.
enum JoinResponse {
    Completed(Box<io::Result<JoinReply>>),
    RefreshInventory,
}

enum SimulationInput {
    Join {
        profile: u128,
        inventory: Inventory,
        sender: SyncSender<OutboundFrame>,
        socket: TcpStream,
        reply: SyncSender<JoinResponse>,
    },
    Command {
        id: u64,
        sequence: u64,
        message: ClientMessage,
    },
    Leave {
        id: u64,
        sequence: u64,
    },
}

pub fn run_server(addr: &str, seed: u64, save_dir: PathBuf) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    let state = server_state(seed, save_dir)?;
    serve_listener(listener, state)
}

pub fn start_local_server(
    seed: u64,
    save_dir: PathBuf,
) -> io::Result<(SocketAddr, thread::JoinHandle<io::Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let state = server_state(seed, save_dir)?;
    let handle = thread::spawn(move || serve_listener(listener, state));
    Ok((addr, handle))
}

fn server_state(seed: u64, save_dir: PathBuf) -> io::Result<State> {
    let mut world = World::with_capacity(seed, save_dir.clone(), SERVER_CHUNK_CACHE)?;
    let inventory_store = InventoryStore::new(&save_dir)?;
    let mut drops = Drops::open(&save_dir)?;
    let durability = Durability::open(&save_dir, &mut world, &inventory_store, &mut drops)?;
    // Only startup may synchronously load the origin terrain. Each live join
    // validates against resident authoritative chunks and defers cache misses.
    let spawn_anchor = spawn_position(&mut world)?;
    let phase_plan = builtin_phase_plan()?;
    let loader = ChunkLoader::new(seed, save_dir, LOADER_CAPACITY)?;
    let worker_count = thread::available_parallelism()
        .map_or(2, |count| count.get())
        .clamp(1, 8);
    let movement_executor = PhaseExecutor::new(worker_count, 64, 64)
        .map_err(|error| io::Error::other(format!("movement worker pool: {error:?}")))?;
    Ok(State {
        world,
        inventory_store,
        drops,
        seed,
        clients: HashMap::new(),
        next_id: 1,
        last_drop_save: Instant::now(),
        moving_drops_dirty: false,
        drops_landed_dirty: false,
        pending_block_changes: Vec::new(),
        durability,
        phase_plan,
        loader,
        movement_executor,
        metrics: MetricsRecorder::new(),
        stream_cursor: 0,
        spawn_anchor,
        pending_joins: VecDeque::new(),
        tick_backlog: 0,
        outbound: Arc::new(OutboundTelemetry::default()),
        last_sent_bytes: 0,
        last_rejections: 0,
    })
}

fn join_client(
    state: &mut State,
    profile: u128,
    loaded_inventory: Inventory,
    sender: SyncSender<OutboundFrame>,
    socket: &TcpStream,
) -> io::Result<JoinReply> {
    if state.durability.failed {
        return Err(io::Error::new(
            ErrorKind::ConnectionAborted,
            "durability subsystem failed; restart required",
        ));
    }
    if state.clients.len() >= MAX_CLIENTS {
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
    let position = spawn_position_cached(state)?;
    let id = state.next_id;
    state.next_id = state
        .next_id
        .checked_add(1)
        .ok_or_else(|| io::Error::other("player ID exhausted"))?;
    let center = world_to_chunk(0, position[1] as i32, 0).0;
    let socket = socket.try_clone()?;
    state.clients.insert(
        id,
        Client {
            profile,
            inventory: inventory.clone(),
            last_drops_revision: u64::MAX,
            last_drop_anchor: [i32::MAX; 3],
            last_sent_drops: Vec::new(),
            sender,
            outbound: Arc::clone(&state.outbound),
            socket,
            sent: HashSet::new(),
            center,
            radius: DEFAULT_VIEW,
            movement: MovementState::new(position, 0),
            pending_moves: VecDeque::new(),
        },
    );
    Ok(JoinReply {
        id,
        seed: state.seed,
        position,
        inventory,
    })
}

fn handle_message(state: &mut State, id: u64, message: ClientMessage) -> io::Result<()> {
    match message {
        ClientMessage::Hello { .. } => {
            Err(io::Error::new(ErrorKind::InvalidData, "duplicate Hello"))
        }
        ClientMessage::Ping { nonce } => {
            if let Some(client) = state.clients.get(&id) {
                client.enqueue(ServerMessage::Pong { nonce });
            }
            Ok(())
        }
        ClientMessage::SetView { radius } => {
            if let Some(client) = state.clients.get_mut(&id) {
                client.radius = radius.clamp(MIN_VIEW_DISTANCE, MAX_VIEW_DISTANCE);
                client.enqueue(ServerMessage::ViewDistance {
                    radius: client.radius,
                });
            }
            Ok(())
        }
        ClientMessage::Resync { key } => {
            if let Some(client) = state.clients.get_mut(&id)
                && client.interested(key)
            {
                client.sent.remove(&key);
            }
            Ok(())
        }
        ClientMessage::Move { seq, dx, dy, dz } => queue_move(state, id, seq, [dx, dy, dz]),
        ClientMessage::Edit { .. }
        | ClientMessage::InventoryMove { .. }
        | ClientMessage::DropStack { .. } => Err(io::Error::new(
            ErrorKind::InvalidData,
            "durable action bypassed coordinator staging",
        )),
    }
}

fn queue_move(state: &mut State, id: u64, seq: u64, delta: [f32; 3]) -> io::Result<()> {
    let Some(client) = state.clients.get_mut(&id) else {
        return Ok(());
    };
    if seq <= client.movement.last_seq()
        || client
            .pending_moves
            .back()
            .is_some_and(|pending| seq <= pending.seq)
    {
        return Ok(());
    }
    if client.pending_moves.len() == MOVEMENT_QUEUE_CAPACITY {
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "movement input queue full",
        ));
    }
    client
        .pending_moves
        .push_back(MovementCommand { seq, delta });
    Ok(())
}

fn block_intersects_player(block: [i32; 3], player: [f32; 3]) -> bool {
    let [x, y, z] = block.map(|n| n as f32);
    x < player[0] + 0.3
        && x + 1.0 > player[0] - 0.3
        && y < player[1] + 1.75
        && y + 1.0 > player[1] + 0.05
        && z < player[2] + 0.3
        && z + 1.0 > player[2] - 0.3
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;

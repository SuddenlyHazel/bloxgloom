//! Authoritative TCP server. The simulation coordinator owns mutable world,
//! inventory, and drop state; chunk loading and journal fsync run on bounded
//! workers. Durable actions become visible only after their WAL receipt.
mod block_actions;
mod builtins;
mod checkpoint;
mod checkpoint_stream;
mod chunk_loader;
mod drops;
mod durable;
mod effects;
mod entities;
mod entity_checkpoint;
mod fire;
mod interest;
mod journal;
mod lifecycle;
mod loot;
mod metrics;
mod movement;
mod net;
mod outbound;
mod parallel;
mod perf;
mod position_store;
mod registry;
mod runtime;
mod simulation;
mod spawn;
mod startup;
mod streaming;
mod voxel_view;

pub use perf::{run_fire_cpu_perf, run_fire_perf, run_perf_benchmark, run_tcp_perf};

use crate::inventory::{Inventory, InventoryStore};
#[cfg(test)]
use crate::protocol;
use crate::protocol::{
    ClientMessage, DroppedItem, MAX_VIEW_DISTANCE, MIN_VIEW_DISTANCE, ServerMessage,
};
use crate::world::{AIR, BEDROCK_Y, ChunkKey, World, world_to_chunk};
use block_actions::BlockActionRegistry;
use chunk_loader::ChunkLoader;
use durable::{
    Durability, handle_live_message, process_durable_actions, publish_committed,
    queue_interaction_actions,
};
use effects::{CellCoord, EffectKindRegistryFrozen};
use entities::{EntityCommit, EntityDelta, EntityStore, PlayerEntityStore};
use fire::FireRuntime;
use metrics::{MetricsRecorder, TickSample};
use movement::{MovementBatch, MovementCommand, MovementState};
use net::serve_listener;
use outbound::{OutboundQueue, OutboundTelemetry};
use parallel::PhaseExecutor;
use position_store::PositionStore;
use registry::PhasePlan;
use runtime::run_simulation_ticks;
use runtime::systems::SystemRuntime;
#[cfg(test)]
use runtime::tick_once;
#[cfg(test)]
use runtime::tick_with_inputs;
use simulation::{FIXED_STEP, Phase, TickId};
#[cfg(test)]
use spawn::collides;
use spawn::{spawn_position, spawn_position_cached};
use startup::ServerStartup;
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

// Admission is deliberately separate from the measured throughput target.
// The foundation gate still requires 128 *real TCP clients* to pass a paced
// soak before we claim this capacity performs well.
const DEFAULT_CLIENTS: usize = 128;
const MAX_CLIENTS: usize = 256;
const OUTBOUND_CAPACITY: usize = 128;
const DEFAULT_VIEW: u8 = 3;
const INPUT_CAPACITY: usize = 8192;
const MAX_INPUTS_PER_TICK: usize = 1024;
const MAX_CATCH_UP_TICKS: usize = 3;
const EDIT_REACH: f32 = 8.0;
// 128 spread clients at the default 7×3×7 view can request more than 16,384
// distinct chunks even before edits or loading overlap.
const SERVER_CHUNK_CACHE: usize = 65_536;
const LOADER_CAPACITY: usize = 256;
const MOVEMENT_QUEUE_CAPACITY: usize = 256;

struct Client {
    profile: u128,
    inventory: Inventory,
    last_drops_revision: u64,
    last_drop_anchor: [i32; 3],
    last_sent_drops: Vec<DroppedItem>,
    sender: OutboundQueue,
    socket: TcpStream,
    sent: streaming::shared::Shared<HashSet<ChunkKey>>,
    sent_epochs: streaming::shared::Shared<HashMap<ChunkKey, u64>>,
    sent_block_versions: streaming::shared::Shared<HashMap<ChunkKey, u64>>,
    sent_entity_revisions: streaming::shared::Shared<HashMap<ChunkKey, u64>>,
    next_snapshot_epoch: u64,
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
        match self.sender.try_send(message) {
            Ok(()) => true,
            Err(error) => {
                let queued = self.sender.snapshot();
                eprintln!(
                    "disconnecting client: outbound {error:?}, {} frames / {} bytes queued",
                    queued.queued_frames, queued.queued_bytes
                );
                let _ = self.socket.shutdown(Shutdown::Both);
                false
            }
        }
    }

    fn interested(&self, key: ChunkKey) -> bool {
        (key.x as i64 - self.center.x as i64).abs() <= self.radius as i64
            && (key.y as i64 - self.center.y as i64).abs()
                <= i64::from(crate::protocol::VERTICAL_VIEW_DISTANCE)
            && (key.z as i64 - self.center.z as i64).abs() <= self.radius as i64
    }
}

struct State {
    admission_limit: usize,
    world: World,
    inventory_store: InventoryStore,
    position_store: PositionStore,
    admin_profile: Option<u128>,
    entities: EntityStore,
    player_entities: PlayerEntityStore,
    /// Frozen notification-effect declarations installed at startup. Entity
    /// plans emit wakes against this registry; delivery only schedules
    /// transient tick attempts and never persists anything.
    effect_kinds: Arc<EffectKindRegistryFrozen>,
    /// Shared per-chunk public entity revision for durable and session-only
    /// changes. It is intentionally separate from the WAL/checkpoint frontier.
    entity_public_revision: u64,
    block_actions: BlockActionRegistry,
    lifecycles: lifecycle::Registry,
    seed: u64,
    clients: HashMap<u64, Client>,
    next_id: u64,
    /// Streaming revision for client drop frames. Bumped at publication
    /// whenever a committed entity delta touches a drop, so clients rescan
    /// visibility without re-reading the whole entity store every tick.
    drop_revision: u64,
    last_expiry_scan: Instant,
    pending_block_changes: Vec<CellCoord>,
    durability: Durability,
    fire: FireRuntime,
    recovered_tick: u64,
    phase_plan: PhasePlan,
    system_runtime: SystemRuntime,
    loader: ChunkLoader,
    movement_executor: PhaseExecutor<MovementBatch, ()>,
    entity_tick_executor:
        PhaseExecutor<durable::actions::entity::TickWorkerResult, entities::EntityError>,
    entity_tick_dispatch_batch: Option<(simulation::TickId, u16)>,
    snapshot_workers: streaming::snapshots::Workers,
    publication_workers: streaming::workers::Workers,
    metrics: MetricsRecorder,
    /// Optional bounded, nonblocking trace for the production-TCP soak.
    /// The live server leaves this absent; the benchmark must drain it.
    tick_observer: Option<SyncSender<TickSample>>,
    stream_cursor: u64,
    spawn_anchor: [f32; 3],
    pending_joins: VecDeque<PendingJoin>,
    tick_backlog: u64,
    outbound: Arc<OutboundTelemetry>,
    last_sent_bytes: u64,
    last_rejections: u64,
}

impl State {
    pub(super) fn advance_entity_public_revision(&mut self) -> io::Result<u64> {
        self.entity_public_revision = self
            .entity_public_revision
            .checked_add(1)
            .ok_or_else(|| io::Error::other("public entity revision exhausted"))?;
        Ok(self.entity_public_revision)
    }

    pub(super) fn queue_player_entity_deltas(
        &mut self,
        deltas: Vec<EntityDelta>,
    ) -> io::Result<()> {
        if deltas.is_empty() {
            return Ok(());
        }
        let registry_revision = self.advance_entity_public_revision()?;
        self.durability.publish_queue.push(durable::PublishEffects {
            client_id: None,
            profile: None,
            action_id: None,
            accepted: true,
            reason: String::new(),
            inventory: None,
            deltas: Vec::new(),
            entity_commit: Some(EntityCommit {
                registry_revision,
                deltas,
            }),
            pickups: Vec::new(),
        });
        Ok(())
    }

    /// Release every authoritative interest pin with the session. Other
    /// clients' subscriptions keep their own pins on shared chunks.
    fn remove_client(&mut self, id: u64) -> Option<Client> {
        let client = self.clients.remove(&id)?;
        if let Err(error) = self.position_store.save(client.profile, client.position()) {
            self.durability.failed = true;
            eprintln!("could not save player position for session {id}: {error}");
        }
        for &key in client.sent.iter() {
            let released = self.world.unpin_resident_chunk(key);
            debug_assert!(released, "client subscription lost its resident chunk");
        }
        match self.player_entities.despawn_session(id) {
            Ok(Some(delta)) => {
                if let Err(error) = self.queue_player_entity_deltas(vec![delta]) {
                    self.durability.failed = true;
                    eprintln!("could not publish player despawn for session {id}: {error}");
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.durability.failed = true;
                eprintln!("could not remove player entity for session {id}: {error}");
            }
        }
        Some(client)
    }

    fn save_connected_positions(&self) -> io::Result<()> {
        for client in self.clients.values() {
            self.position_store
                .save(client.profile, client.position())?;
        }
        Ok(())
    }
}

struct PendingJoin {
    profile: u128,
    inventory: Inventory,
    sender: OutboundQueue,
    socket: TcpStream,
    reply: SyncSender<JoinResponse>,
    queued_at: Instant,
}

struct JoinReply {
    id: u64,
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
        inventory: Box<Inventory>,
        sender: OutboundQueue,
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
    run_server_with_limit(addr, seed, save_dir, DEFAULT_CLIENTS)
}

/// Development installation seam shared by local server and client catalogs.
#[cfg(feature = "lifecycle-fixture")]
pub(crate) fn catalog_with_extension(
    mut catalog: crate::content::Catalog,
    extension: &dyn bloxgloom_host_api::Extension,
) -> io::Result<crate::content::Catalog> {
    lifecycle::Registration::install(extension, &mut catalog).map_err(io::Error::other)?;
    Ok(catalog)
}

pub fn run_server_with_limit(
    addr: &str,
    seed: u64,
    save_dir: PathBuf,
    admission_limit: usize,
) -> io::Result<()> {
    run_server_with_limit_and_catalog(
        addr,
        seed,
        save_dir,
        admission_limit,
        Arc::new(crate::content::catalog().clone()),
    )
}

/// Starts one world with its frozen content definitions. The listener,
/// storage, inventory, entity registry, and handshake all derive from this
/// world catalog rather than looking up process-global content independently.
pub fn run_server_with_limit_and_catalog(
    addr: &str,
    seed: u64,
    save_dir: PathBuf,
    admission_limit: usize,
    catalog: Arc<crate::content::Catalog>,
) -> io::Result<()> {
    run_server_with_startup(
        addr,
        seed,
        save_dir,
        admission_limit,
        ServerStartup::new(catalog),
    )
}

pub(crate) fn run_server_with_startup(
    addr: &str,
    seed: u64,
    save_dir: PathBuf,
    admission_limit: usize,
    startup: ServerStartup,
) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    let state = server_state_with_startup(seed, save_dir, admission_limit, startup)?;
    serve_listener(listener, Box::new(state))
}

#[cfg(test)]
pub fn start_local_server(seed: u64, save_dir: PathBuf) -> io::Result<(SocketAddr, LocalServer)> {
    start_local_server_for_profile(seed, save_dir, None)
}

pub fn start_local_server_with_admin(
    seed: u64,
    save_dir: PathBuf,
    admin_profile: u128,
) -> io::Result<(SocketAddr, LocalServer)> {
    if admin_profile == 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "missing admin profile",
        ));
    }
    start_local_server_for_profile(seed, save_dir, Some(admin_profile))
}

fn start_local_server_for_profile(
    seed: u64,
    save_dir: PathBuf,
    admin_profile: Option<u128>,
) -> io::Result<(SocketAddr, LocalServer)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let mut state = server_state(seed, save_dir)?;
    state.admin_profile = admin_profile;
    let state = Box::new(state);
    let (stop, receiver) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        net::serve_listener_with_stats(
            listener,
            state,
            receiver,
            Arc::new(net::TransportStats::default()),
        )
    });
    Ok((addr, LocalServer { stop, handle }))
}

pub struct LocalServer {
    stop: std::sync::mpsc::Sender<()>,
    handle: thread::JoinHandle<io::Result<()>>,
}

impl LocalServer {
    pub fn stop(self) -> io::Result<()> {
        let _ = self.stop.send(());
        self.handle
            .join()
            .map_err(|_| io::Error::other("local server panicked"))?
    }
}

fn server_state(seed: u64, save_dir: PathBuf) -> io::Result<State> {
    server_state_with_limit(seed, save_dir, DEFAULT_CLIENTS)
}

fn server_state_with_limit(
    seed: u64,
    save_dir: PathBuf,
    admission_limit: usize,
) -> io::Result<State> {
    server_state_with_limit_and_catalog(
        seed,
        save_dir,
        admission_limit,
        Arc::new(crate::content::catalog().clone()),
    )
}

fn server_state_with_limit_and_catalog(
    seed: u64,
    save_dir: PathBuf,
    admission_limit: usize,
    catalog: Arc<crate::content::Catalog>,
) -> io::Result<State> {
    server_state_with_startup(seed, save_dir, admission_limit, ServerStartup::new(catalog))
}

fn server_state_with_startup(
    seed: u64,
    save_dir: PathBuf,
    admission_limit: usize,
    startup: ServerStartup,
) -> io::Result<State> {
    if !(1..=MAX_CLIENTS).contains(&admission_limit) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "admission limit must be 1..=256",
        ));
    }
    let phase_plan = startup.phase_plan()?;
    let catalog = startup.catalog();
    // A missing entity codec is a startup error, not a reason to create or
    // rewrite content.map before rejecting the world. Repeat after loading
    // because an existing world's manifest may resolve numeric IDs.
    let _ = startup.entity_types_for(Arc::clone(&catalog))?;
    let _ = startup.block_actions_for(&catalog)?;
    let mut world =
        World::with_capacity_and_catalog(seed, save_dir.clone(), SERVER_CHUNK_CACHE, catalog)?;
    let inventory_store = InventoryStore::with_catalog(&save_dir, world.catalog_arc())?;
    let position_store = PositionStore::new(&save_dir)?;
    let catalog = world.catalog_arc();
    let entity_types = startup.entity_types_for(catalog.clone())?;
    let effect_kinds = Arc::new(startup.effect_kinds()?);
    let (block_actions, lifecycles) = startup.block_actions_for(&catalog)?;
    let owner_configs = startup.owner_configs()?;
    let (mut durability, recovered_fire, entities, owner_store, wake_store, cursors) =
        Durability::open(
            &save_dir,
            &mut world,
            &inventory_store,
            entity_types,
            owner_configs,
        )?;
    let recovered_tick = durability.recovered_tick.max(recovered_fire.last_tick());
    // Only startup may synchronously load the origin terrain. Each live join
    // validates against resident authoritative chunks and defers cache misses.
    let spawn_anchor = spawn_position(&mut world)?;
    let loader = ChunkLoader::new(&world, LOADER_CAPACITY)?;
    let worker_count = thread::available_parallelism()
        .map_or(2, |count| count.get())
        .clamp(1, 8);
    let fire = FireRuntime::new(recovered_fire, worker_count)?;
    // One active movement job and one result can be produced per client in a
    // tick. Leave headroom for barrier hand-off without imposing a 64-player
    // failure threshold below admission capacity.
    let movement_executor =
        PhaseExecutor::new(worker_count, admission_limit * 2, admission_limit * 2)
            .map_err(|error| io::Error::other(format!("movement worker pool: {error:?}")))?;
    let entity_tick_executor = PhaseExecutor::new(
        worker_count,
        durable::MAX_PENDING_DURABLE_ACTIONS,
        durable::MAX_PENDING_DURABLE_ACTIONS,
    )
    .map_err(|error| io::Error::other(format!("entity tick worker pool: {error:?}")))?;
    let mut system_runtime =
        SystemRuntime::with_durable_store(worker_count, owner_store, wake_store, cursors)?;
    startup.install_owners(&mut system_runtime, &mut durability)?;
    let entity_public_revision = entities.revision();
    Ok(State {
        admission_limit,
        world,
        inventory_store,
        position_store,
        admin_profile: None,
        entities,
        player_entities: PlayerEntityStore::default(),
        effect_kinds,
        entity_public_revision,
        block_actions,
        lifecycles,
        seed,
        clients: HashMap::new(),
        next_id: 1,
        drop_revision: 0,
        last_expiry_scan: Instant::now(),
        pending_block_changes: Vec::new(),
        durability,
        fire,
        recovered_tick,
        phase_plan,
        system_runtime,
        loader,
        movement_executor,
        entity_tick_executor,
        entity_tick_dispatch_batch: None,
        snapshot_workers: streaming::snapshots::Workers::new(worker_count)?,
        publication_workers: streaming::workers::Workers::new(worker_count)?,
        metrics: MetricsRecorder::new(),
        tick_observer: None,
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
    let position = match state.position_store.load(profile)? {
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
    let id = state.next_id;
    let next_id = state
        .next_id
        .checked_add(1)
        .ok_or_else(|| io::Error::other("player ID exhausted"))?;
    let socket = socket.try_clone()?;
    let (owned_entity_id, spawn_delta) = state
        .player_entities
        .spawn_session(id, position)
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
    ] {
        if sender.try_send(message).is_err() {
            state.player_entities.discard_session(id);
            return Err(io::Error::new(
                ErrorKind::BrokenPipe,
                "client startup queue closed",
            ));
        }
    }
    state.next_id = next_id;
    state.clients.insert(
        id,
        Client {
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

fn handle_message(state: &mut State, id: u64, message: ClientMessage) -> io::Result<()> {
    match message {
        ClientMessage::Hello { .. } => {
            Err(io::Error::new(ErrorKind::InvalidData, "duplicate Hello"))
        }
        ClientMessage::ContentReady { .. } => Err(io::Error::new(
            ErrorKind::InvalidData,
            "duplicate content readiness",
        )),
        ClientMessage::Ping { nonce } => {
            if let Some(client) = state.clients.get(&id) {
                client.enqueue(ServerMessage::Pong { nonce });
            }
            Ok(())
        }
        ClientMessage::SetView { radius } => {
            let requested = radius.clamp(MIN_VIEW_DISTANCE, MAX_VIEW_DISTANCE);
            let can_expand = state.world.can_admit_chunk();
            if let Some(client) = state.clients.get_mut(&id) {
                if requested <= client.radius || can_expand {
                    client.radius = requested;
                }
                client.enqueue(ServerMessage::ViewDistance {
                    radius: client.radius,
                });
            }
            Ok(())
        }
        ClientMessage::Resync { key } => {
            let mut removed = false;
            if let Some(client) = state.clients.get_mut(&id)
                && client.interested(key)
            {
                removed = client.sent.remove(&key);
                client.sent_epochs.remove(&key);
                client.sent_block_versions.remove(&key);
                client.sent_entity_revisions.remove(&key);
            }
            if removed {
                let released = state.world.unpin_resident_chunk(key);
                debug_assert!(released, "resync subscription lost its resident chunk");
            }
            Ok(())
        }
        ClientMessage::Move { seq, dx, dy, dz } => queue_move(state, id, seq, [dx, dy, dz]),
        ClientMessage::Edit { .. }
        | ClientMessage::InventoryMove { .. }
        | ClientMessage::DropStack { .. }
        | ClientMessage::AdminGive { .. }
        | ClientMessage::AdminSpawnEntity { .. }
        | ClientMessage::EntityInteract { .. }
        | ClientMessage::ActionAck { .. } => Err(io::Error::new(
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

//! Authoritative TCP server. The simulation coordinator owns mutable world,
//! inventory, and drop state; chunk loading and journal fsync run on bounded
//! workers. Durable actions become visible only after their WAL receipt.
mod appearance;
mod block_actions;
mod builtins;
mod checkpoint;
mod checkpoint_stream;
mod chunk_loader;
mod drops;
mod durable;
mod effects;
mod entities;
mod session_ids;
mod entity_checkpoint;
mod fire;
mod gameplay;
mod interest;
mod inventory_loading;
mod journal;
mod lifecycle;
#[cfg(test)]
mod loot;
mod metrics;
mod movement;
mod net;
mod notifications;
mod outbound;
mod parallel;
mod perf;
mod players;
use players::admission::join_named_client;
mod position_store;
mod registry;
mod runtime;
// Phase 4 foundation: not dispatched by gameplay yet.
#[allow(dead_code)]
mod script;
pub(crate) use script::package::PackageSnapshot;
pub(crate) use script::package::client as client_bundle;
pub(crate) use script::runtime as script_runtime;
pub(crate) use script::{SourceModule, capacity as script_capacity, handles as script_handles};
mod simulation;
mod spawn;
mod startup;

pub(crate) fn package_catalog_for_preview(
    catalog: crate::content::Catalog,
    root: &std::path::Path,
) -> std::io::Result<crate::content::Catalog> {
    startup::ServerStartup::new(std::sync::Arc::new(catalog))
        .with_local_packages(root)
        .map(startup::ServerStartup::into_preview_catalog)
}
pub(crate) fn package_bundle_for_preview(
    root: &std::path::Path,
) -> std::io::Result<Arc<client_bundle::ClientBundle>> {
    let startup = startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_local_packages(root)?;
    startup
        .client_bundle
        .ok_or_else(|| io::Error::other("missing prepared package bundle"))
}
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
mod world_time;
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
    name: String,
    action_epoch: u64,
    last_roster_revision: u64,
    last_player_state_revision: u64,
    last_player_states: Option<Vec<crate::protocol::PlayerState>>,
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
    movement_reset: movement::Reset,
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
                tracing::warn!(
                    ?error,
                    queued_frames = queued.queued_frames,
                    queued_bytes = queued.queued_bytes,
                    "disconnecting client after outbound queue failure"
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
    client_bundle: Option<Arc<script::package::client::ClientBundle>>,
    notifications: notifications::Lane,
    admission_limit: usize,
    world: World,
    inventory_store: InventoryStore,
    profile_inventory_cache: players::inventory::Cache,
    position_store: PositionStore,
    appearance_store: appearance::Store,
    admin_profile: Option<u128>,
    entities: EntityStore,
    player_entities: PlayerEntityStore,
    player_runtime: players::Runtime,
    roster_revision: u64,
    player_state_revision: u64,
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
    world_time: world_time::Clock,
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
            fire_bursts: Vec::new(),
        });
        Ok(())
    }

    /// Release every authoritative interest pin with the session. Other
    /// clients' subscriptions keep their own pins on shared chunks.
    fn remove_client(&mut self, id: u64) -> Option<Client> {
        players::leaving(self, id);
        let client = self.clients.remove(&id)?;
        if client.movement.crouching() {
            movement::clear_stance(self, id);
        }
        if let Err(error) = self.position_store.save(client.profile, client.position()) {
            self.durability.failed = true;
            tracing::error!(%error, player_id = id, "player position save failed");
        }
        for &key in client.sent.iter() {
            let released = self.world.unpin_resident_chunk(key);
            debug_assert!(released, "client subscription lost its resident chunk");
        }
        match self.player_entities.despawn_session(id) {
            Ok(Some(delta)) => {
                if let Err(error) = self.queue_player_entity_deltas(vec![delta]) {
                    self.durability.failed = true;
                    tracing::error!(%error, player_id = id, "player despawn publication failed");
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.durability.failed = true;
                tracing::error!(%error, player_id = id, "player entity removal failed");
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
    guard: players::JoinGuard,
    name: String,
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
        guard: players::JoinGuard,
        name: String,
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

/// Development-only opt-in; never searches the working directory or save tree.
pub fn run_server_with_local_packages(
    addr: &str,
    seed: u64,
    save_dir: PathBuf,
    admission_limit: usize,
    package_root: &std::path::Path,
) -> io::Result<()> {
    let startup = ServerStartup::new(Arc::new(crate::content::catalog().clone()))
        .with_local_packages(package_root)?;
    run_server_with_startup(addr, seed, save_dir, admission_limit, startup)
}

/// Development installation seam shared by local server and client catalogs.
#[cfg(any(test, feature = "lifecycle-fixture"))]
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

/// Local package-development path with the same frozen startup and admin
/// profile used by `local`, but without reusing a builtin-only save.
pub fn start_local_server_with_packages(
    seed: u64,
    save_dir: PathBuf,
    admin_profile: u128,
    package_root: &std::path::Path,
) -> io::Result<(SocketAddr, LocalServer)> {
    if admin_profile == 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "missing admin profile",
        ));
    }
    let startup = ServerStartup::new(Arc::new(crate::content::catalog().clone()))
        .with_local_packages(package_root)?;
    start_local_server_with_startup(seed, save_dir, Some(admin_profile), startup)
}

fn start_local_server_for_profile(
    seed: u64,
    save_dir: PathBuf,
    admin_profile: Option<u128>,
) -> io::Result<(SocketAddr, LocalServer)> {
    let startup = ServerStartup::new(Arc::new(crate::content::catalog().clone()));
    start_local_server_with_startup(seed, save_dir, admin_profile, startup)
}

fn start_local_server_with_startup(
    seed: u64,
    save_dir: PathBuf,
    admin_profile: Option<u128>,
    startup: ServerStartup,
) -> io::Result<(SocketAddr, LocalServer)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let mut state = server_state_with_startup(seed, save_dir, DEFAULT_CLIENTS, startup)?;
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
    let mut world = World::with_generation(
        seed,
        save_dir.clone(),
        SERVER_CHUNK_CACHE,
        catalog,
        startup.generation(),
    )?;
    let inventory_store = InventoryStore::with_catalog(&save_dir, world.catalog_arc())?;
    let position_store = PositionStore::new(&save_dir)?;
    let appearance_store = appearance::Store::new(&save_dir)?;
    let catalog = world.catalog_arc();
    let notifications = notifications::Lane::new(&catalog)?;
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
    let world_time = world_time::Clock::open(&save_dir)?;
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
    let client_bundle = startup.client_bundle.clone();
    startup.install_owners(&mut system_runtime, &mut durability)?;
    let entity_public_revision = entities.revision();
    let first_session_id = session_ids::reserve(&save_dir)?;
    Ok(State {
        client_bundle,
        notifications,
        admission_limit,
        world,
        profile_inventory_cache: players::inventory::Cache::new(inventory_store.clone()),
        inventory_store,
        position_store,
        appearance_store,
        admin_profile: None,
        entities,
        player_entities: PlayerEntityStore::default(),
        player_runtime: players::Runtime::default(),
        roster_revision: 1,
        player_state_revision: 1,
        effect_kinds,
        entity_public_revision,
        block_actions,
        lifecycles,
        seed,
        clients: HashMap::new(),
        next_id: first_session_id,
        drop_revision: 0,
        last_expiry_scan: Instant::now(),
        pending_block_changes: Vec::new(),
        durability,
        fire,
        recovered_tick,
        world_time,
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

fn handle_message(state: &mut State, id: u64, message: ClientMessage) -> io::Result<()> {
    match message {
        ClientMessage::SetWorldTime { .. } => Err(io::Error::new(
            ErrorKind::InvalidInput,
            "time command requires durable dispatch",
        )),
        ClientMessage::SetCrouching { crouching } => {
            if let Some(client) = state.clients.get_mut(&id) {
                client.movement.request_crouch(crouching);
            }
            Ok(())
        }
        ClientMessage::SelectAppearance { palettes } => appearance::select(state, id, palettes),
        ClientMessage::SelectCharacter { recipe } => {
            appearance::select_character(state, id, recipe)
        }
        ClientMessage::Hello { .. } => {
            Err(io::Error::new(ErrorKind::InvalidData, "duplicate Hello"))
        }
        ClientMessage::ContentReady { .. }
        | ClientMessage::BundleRequest { .. }
        | ClientMessage::BundleReady { .. } => Err(io::Error::new(
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
        ClientMessage::MovementReady {
            session,
            reset,
            next_seq,
        } => movement::movement_ready(state, id, session, reset, next_seq),
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
    if client.movement_reset.pending {
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

fn block_intersects_player(
    body: bloxgloom_host_api::player::Body,
    block: [i32; 3],
    player: [f32; 3],
) -> bool {
    body.intersects_block(block, player)
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;

#[cfg(test)]
fn join_client(
    state: &mut State,
    profile: u128,
    action_epoch: u64,
    inventory: Inventory,
    sender: OutboundQueue,
    socket: &TcpStream,
) -> io::Result<JoinReply> {
    join_named_client(
        state,
        profile,
        &format!("player-{profile:x}"),
        action_epoch,
        inventory,
        sender,
        socket,
    )
}

//! Deterministic world, player, drop, and input fixtures for server benchmarks.

use super::super::movement::MovementState;
use super::super::outbound::OutboundFrame;
use super::super::{Client, SimulationInput, State};
use crate::inventory::{Inventory, SLOTS, Stack};
use crate::items::ItemId;
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::world::{
    AIR, CHUNK_SIZE, ChunkKey, DIRT, MAX_GENERATED_HEIGHT, STONE, World, is_solid, terrain_height,
    world_to_chunk,
};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) const PLAYER_COUNT: usize = 16;
pub(super) const MIN_STEADY_TICKS: usize = 300;
pub(super) const MAX_STEADY_TICKS: usize = 15_000;
const STREAM_RADIUS: u8 = 2;
pub(super) const ACTION_INTERVAL: usize = 30;
pub(super) const DROP_HEIGHTS: [f32; 4] = [0.0, 70.0, 140.0, 210.0];
pub(super) const SEED: u64 = 0xB10C_6100;
const STONE_ITEM: ItemId = ItemId::new(STONE.get());
const DIRT_ITEM: ItemId = ItemId::new(DIRT.get());

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
pub(super) enum Scenario {
    Clustered,
    Spread,
}

impl Scenario {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Clustered => "clustered",
            Self::Spread => "spread",
        }
    }

    pub(super) fn rough_positions(self) -> [[f32; 3]; PLAYER_COUNT] {
        std::array::from_fn(|index| match self {
            Self::Clustered => {
                let column = (index % 4) as f32;
                let row = (index / 4) as f32;
                [2.5 + column * 3.0, 0.0, 2.5 + row * 3.0]
            }
            Self::Spread => {
                const CHUNKS: [i32; 4] = [-6, -2, 2, 6];
                let chunk_x = CHUNKS[index % 4];
                let chunk_z = CHUNKS[index / 4];
                [
                    (chunk_x * CHUNK_SIZE as i32 + 8) as f32,
                    0.0,
                    (chunk_z * CHUNK_SIZE as i32 + 8) as f32,
                ]
            }
        })
    }
}

pub(super) struct TempSaveDir {
    pub(super) path: PathBuf,
}

impl TempSaveDir {
    pub(super) fn create() -> io::Result<Self> {
        let unix_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-server-perf-{}-{unix_nanos}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempSaveDir {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            eprintln!("server-perf: failed to remove temporary save: {error}");
        }
    }
}

pub(super) struct ScenarioSetup {
    pub(super) receivers: Vec<Receiver<OutboundFrame>>,
    /// Keep the client ends alive; this harness drains the authoritative
    /// outbound queues without doing actual TCP writes.
    _peers: Vec<TcpStream>,
    block_targets: Vec<[i32; 3]>,
}

#[derive(Default)]
pub(super) struct DrainTotals {
    pub(super) sent_frames: u64,
    pub(super) sent_bytes: u64,
    pub(super) accepted_actions: u64,
    pub(super) rejected_actions: u64,
}

pub(super) fn prepare(state: &mut State, scenario: Scenario) -> io::Result<(ScenarioSetup, usize)> {
    let rough_positions = scenario.rough_positions();
    warm_surface_chunks(state, &rough_positions)?;
    let positions = surface_positions(&state.world, &rough_positions)?;
    let warmed_chunks = warm_runtime_chunks(state, &positions)?;
    let setup = add_clients_and_seed_drops(state, positions)?;
    Ok((setup, warmed_chunks))
}

fn warm_surface_chunks(state: &mut State, positions: &[[f32; 3]; PLAYER_COUNT]) -> io::Result<()> {
    let mut keys = HashSet::new();
    for position in positions {
        let center = world_to_chunk(position[0].floor() as i32, 0, position[2].floor() as i32).0;
        // The supported terrain surface and the three-block edit target fit
        // in these vertical chunks. Candidate positions stay in this x/z chunk.
        for y in 0..=5 {
            keys.insert(ChunkKey {
                x: center.x,
                y,
                z: center.z,
            });
        }
    }
    install_chunks(state, keys)
}

fn warm_runtime_chunks(
    state: &mut State,
    positions: &[[f32; 3]; PLAYER_COUNT],
) -> io::Result<usize> {
    let mut keys = HashSet::new();
    let highest_drop_y = MAX_GENERATED_HEIGHT + 1 + DROP_HEIGHTS[DROP_HEIGHTS.len() - 1] as i32;
    let top_chunk_y = world_to_chunk(0, highest_drop_y, 0).0.y;
    for position in positions {
        let center = world_to_chunk(
            position[0].floor() as i32,
            position[1].floor() as i32,
            position[2].floor() as i32,
        )
        .0;
        // Player movement dispatch captures a 3x3x3 resident neighborhood.
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    keys.insert(ChunkKey {
                        x: center.x + dx,
                        y: center.y + dy,
                        z: center.z + dz,
                    });
                }
            }
        }
        // Seeded drops fall down this exact surface column through the run.
        for y in 0..=top_chunk_y {
            keys.insert(ChunkKey {
                x: center.x,
                y,
                z: center.z,
            });
        }
    }
    let count = keys.len();
    install_chunks(state, keys)?;
    Ok(count)
}

fn install_chunks(state: &mut State, keys: HashSet<ChunkKey>) -> io::Result<()> {
    let mut ordered: Vec<_> = keys.into_iter().collect();
    ordered.sort_unstable_by_key(|key| (key.x, key.y, key.z));
    for key in &ordered {
        if state.world.cached_version(*key).is_none() {
            let loaded = state.world.load_chunk_uncached(*key)?;
            state.world.install_loaded_if_absent(loaded, 0)?;
        }
    }
    Ok(())
}

fn surface_positions(
    world: &World,
    rough_positions: &[[f32; 3]; PLAYER_COUNT],
) -> io::Result<[[f32; 3]; PLAYER_COUNT]> {
    let mut selected: Vec<[f32; 3]> = Vec::with_capacity(PLAYER_COUNT);
    for rough in rough_positions {
        let base_x = rough[0].floor() as i32;
        let base_z = rough[2].floor() as i32;
        let center = world_to_chunk(base_x, 0, base_z).0;
        let chunk_start_x = center.x * CHUNK_SIZE as i32;
        let chunk_start_z = center.z * CHUNK_SIZE as i32;
        let mut candidates = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE);
        for z in chunk_start_z..chunk_start_z + CHUNK_SIZE as i32 {
            for x in chunk_start_x..chunk_start_x + CHUNK_SIZE as i32 {
                let dx = x - base_x;
                let dz = z - base_z;
                candidates.push((dx * dx + dz * dz, x, z));
            }
        }
        candidates.sort_unstable();
        let mut found = None;
        for (_, x, z) in candidates {
            let top = terrain_height(i64::from(x), i64::from(z), SEED) as i32;
            let Some(support) = world.cached_block(x, top, z) else {
                continue;
            };
            if !is_solid(support)
                || [1, 2, 3]
                    .into_iter()
                    .any(|offset| world.cached_block(x, top + offset, z) != Some(AIR))
            {
                continue;
            }
            let position = [x as f32 + 0.5, top as f32 + 1.0, z as f32 + 0.5];
            if selected.iter().any(|previous| {
                let dx = position[0] - previous[0];
                let dz = position[2] - previous[2];
                dx * dx + dz * dz < 1.44
            }) {
                continue;
            }
            found = Some(position);
            break;
        }
        let position = found.ok_or_else(|| {
            io::Error::other(format!(
                "no safe generated surface near ({base_x}, {base_z})"
            ))
        })?;
        selected.push(position);
    }
    selected
        .try_into()
        .map_err(|_| io::Error::other("server-perf surface fixture count mismatch"))
}

fn add_clients_and_seed_drops(
    state: &mut State,
    positions: [[f32; 3]; PLAYER_COUNT],
) -> io::Result<ScenarioSetup> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let mut receivers = Vec::with_capacity(PLAYER_COUNT);
    let mut peers = Vec::with_capacity(PLAYER_COUNT);
    let mut spawn_requests = Vec::with_capacity(PLAYER_COUNT * DROP_HEIGHTS.len());
    let mut block_targets = Vec::with_capacity(PLAYER_COUNT);

    for (index, position) in positions.into_iter().enumerate() {
        let peer = TcpStream::connect(address)?;
        let (socket, _) = listener.accept()?;
        let (sender, receiver) = state.outbound.client_queue();
        let center = world_to_chunk(
            position[0].floor() as i32,
            position[1].floor() as i32,
            position[2].floor() as i32,
        )
        .0;
        let id = index as u64 + 1;
        let profile = 0xB10C_6100_0000_0000u128 + index as u128 + 1;
        let mut inventory = Inventory::default();
        for slot in 0..SLOTS {
            let item = if slot % 2 == 0 { STONE_ITEM } else { DIRT_ITEM };
            inventory.slots[slot] = Some(Stack::new(item, 128));
        }
        // Durable replay expects the initial before-value to exist in the
        // checkpoint baseline before the first WAL inventory transition.
        let inventory_bytes = crate::inventory::InventoryStore::encode_snapshot(&inventory)?;
        state
            .inventory_store
            .checkpoint_snapshot(profile, &inventory_bytes)?;
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
                radius: STREAM_RADIUS,
                movement: MovementState::new(position, 0),
                pending_moves: Default::default(),
            },
        );
        receivers.push(receiver);
        peers.push(peer);
        block_targets.push([
            position[0].floor() as i32,
            (position[1] + 2.0).floor() as i32,
            position[2].floor() as i32,
        ]);
        for height in DROP_HEIGHTS {
            spawn_requests.push((
                [position[0], position[1] + height, position[2]],
                STONE_ITEM,
                1,
                Duration::ZERO,
            ));
        }
    }
    state.next_id = PLAYER_COUNT as u64 + 1;

    // This is explicit fixture setup on a unique disposable save. The seed
    // stages through the real WAL path so the entity checkpoint mirror
    // replays the same batch as the live store; runtime edits and receipts
    // below then continue through the normal durable flow.
    let catalog = state.world.catalog_arc();
    let spawns = crate::server::drops::plan_spawns(
        &state.entities,
        &catalog,
        &spawn_requests,
        1,
        crate::server::drops::unix_ms(),
    )?
    .ok_or_else(|| io::Error::other("server-perf planned no seed drops"))?;
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()?
        .ok_or_else(|| io::Error::other("server-perf mirror refused the seed"))?;
    let seed = crate::server::durable::CommitAction {
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
        entities: Some(spawns),
    };
    let tick = crate::server::simulation::TickId::new(1);
    if !state
        .durability
        .try_stage(tick, &seed, Some(permit))
        .map_err(|error| io::Error::other(format!("server-perf seed WAL stage: {error:?}")))?
    {
        return Err(io::Error::other("server-perf seed staged no changes"));
    }
    for _ in 0..2_000 {
        crate::server::durable::process_durable_actions(state, tick, std::time::Instant::now())?;
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    if !state.durability.pending.is_empty() {
        return Err(io::Error::other("server-perf seed did not commit"));
    }
    if crate::server::drops::airborne_count(&state.entities) < PLAYER_COUNT {
        return Err(io::Error::other("server-perf failed to seed active drops"));
    }
    Ok(ScenarioSetup {
        receivers,
        _peers: peers,
        block_targets,
    })
}

pub(super) fn tick_inputs(
    state: &State,
    setup: &ScenarioSetup,
    scenario: Scenario,
    tick_number: usize,
) -> io::Result<Vec<SimulationInput>> {
    let mut inputs = Vec::with_capacity(PLAYER_COUNT * 2);
    let sway = if tick_number.is_multiple_of(2) {
        0.08
    } else {
        -0.08
    };
    for index in 0..PLAYER_COUNT {
        let id = index as u64 + 1;
        let sequence = (tick_number as u64).saturating_mul(2);
        inputs.push(SimulationInput::Command {
            id,
            sequence: sequence - 1,
            message: ClientMessage::Move {
                seq: tick_number as u64,
                dx: sway,
                dy: 0.0,
                dz: -sway,
            },
        });
        if !tick_number.is_multiple_of(ACTION_INTERVAL) {
            continue;
        }
        let action_id = ((scenario_id(scenario) as u128) << 120)
            | ((index as u128 + 1) << 64)
            | tick_number as u128;
        let target = setup.block_targets[index];
        let current_block = state.world.cached_block(target[0], target[1], target[2]);
        let action = match (tick_number / ACTION_INTERVAL) % 3 {
            0 => ClientMessage::InventoryMove {
                action_id,
                from: 0,
                to: 1,
                count: 128,
            },
            1 if current_block == Some(AIR) => {
                let inventory = &state.clients[&id].inventory;
                let Some(stack) = inventory.slots[0].as_ref() else {
                    return Err(io::Error::other("server-perf inventory unexpectedly empty"));
                };
                let block = if stack.item == STONE_ITEM {
                    STONE
                } else if stack.item == DIRT_ITEM {
                    DIRT
                } else {
                    return Err(io::Error::other("server-perf selected item is not a block"));
                };
                ClientMessage::Edit {
                    action_id,
                    x: target[0],
                    y: target[1],
                    z: target[2],
                    block,
                    slot: 0,
                }
            }
            2 if current_block.is_some_and(|block| block != AIR) => ClientMessage::Edit {
                action_id,
                x: target[0],
                y: target[1],
                z: target[2],
                block: AIR,
                slot: 0,
            },
            _ => ClientMessage::InventoryMove {
                action_id,
                from: 0,
                to: 1,
                count: 128,
            },
        };
        inputs.push(SimulationInput::Command {
            id,
            sequence,
            message: action,
        });
    }
    Ok(inputs)
}

const fn scenario_id(scenario: Scenario) -> u8 {
    match scenario {
        Scenario::Clustered => 1,
        Scenario::Spread => 2,
    }
}

pub(super) fn drain_outbound(receivers: &[Receiver<OutboundFrame>], totals: &mut DrainTotals) {
    for receiver in receivers {
        loop {
            let frame = match receiver.try_recv() {
                Ok(frame) => frame,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
            match frame.message() {
                ServerMessage::ActionResult { accepted: true, .. } => {
                    totals.accepted_actions += 1;
                }
                ServerMessage::ActionResult {
                    accepted: false, ..
                } => {
                    totals.rejected_actions += 1;
                }
                _ => {}
            }
            // The frame's exact protocol size was reserved by OutboundTelemetry.
            // The benchmark is a headless queue sink, not a TCP throughput test.
            totals.sent_bytes += protocol::server_wire_len(frame.message()) as u64;
            frame.record_sent();
            totals.sent_frames += 1;
            drop(frame);
        }
    }
}

//! Authoritative TCP server. A single world lock orders snapshots and edits for
//! every client. Cold chunk generation and durable edits currently serialize on
//! that lock; the per-client stream rate is bounded, and stream tick timings are
//! reported so this limit is visible under the 16-player target load.
mod drops;

use crate::inventory::{Inventory, InventoryStore};
use crate::protocol::{self, ClientMessage, MAX_VIEW_DISTANCE, MIN_VIEW_DISTANCE, ServerMessage};
#[cfg(test)]
use crate::world::STONE;
use crate::world::{
    AIR, BEDROCK_Y, ChunkKey, MAX_BLOCK, MAX_TERRAIN_HEIGHT, World, world_to_chunk,
};
use drops::Drops;
use std::collections::{HashMap, HashSet};
use std::io::{self, ErrorKind};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const MAX_CLIENTS: usize = 16;
const OUTBOUND_CAPACITY: usize = 128;
const DEFAULT_VIEW: u8 = 3;
const STREAM_INTERVAL: Duration = Duration::from_millis(20);
const PLAYER_SPEED: f32 = 10.0;
const EDIT_REACH: f32 = 8.0;

struct Client {
    profile: u128,
    inventory: Inventory,
    last_drops_revision: u64,
    last_drop_anchor: [i32; 3],
    sender: SyncSender<ServerMessage>,
    socket: TcpStream,
    sent: HashSet<ChunkKey>,
    center: ChunkKey,
    radius: u8,
    position: [f32; 3],
    last_move: Instant,
    last_seq: u64,
}

impl Client {
    fn enqueue(&self, message: ServerMessage) -> bool {
        match self.sender.try_send(message) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                eprintln!("disconnecting slow client: outbound queue full or closed");
                let _ = self.socket.shutdown(Shutdown::Both);
                false
            }
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

fn server_state(seed: u64, save_dir: PathBuf) -> io::Result<Arc<Mutex<State>>> {
    let world = World::new(seed, save_dir.clone())?;
    let inventory_store = InventoryStore::new(&save_dir)?;
    Ok(Arc::new(Mutex::new(State {
        world,
        inventory_store,
        drops: Drops::open(&save_dir)?,
        seed,
        clients: HashMap::new(),
        next_id: 1,
    })))
}

fn serve_listener(listener: TcpListener, state: Arc<Mutex<State>>) -> io::Result<()> {
    let connections = Arc::new(AtomicUsize::new(0));
    eprintln!("Bloxgloom server listening on {}", listener.local_addr()?);
    for connection in listener.incoming() {
        let socket = match connection {
            Ok(socket) => socket,
            Err(error) => {
                eprintln!("accept: {error}");
                continue;
            }
        };
        if connections.fetch_add(1, Ordering::AcqRel) >= MAX_CLIENTS {
            connections.fetch_sub(1, Ordering::AcqRel);
            let _ = socket.shutdown(Shutdown::Both);
            continue;
        }
        if let Err(error) = socket.set_nodelay(true) {
            connections.fetch_sub(1, Ordering::AcqRel);
            eprintln!("client socket: {error}");
            continue;
        }
        let shared = Arc::clone(&state);
        let connections = Arc::clone(&connections);
        thread::spawn(move || {
            if let Err(error) = serve_client(socket, shared) {
                eprintln!("client: {error}");
            }
            connections.fetch_sub(1, Ordering::AcqRel);
        });
    }
    Ok(())
}

fn serve_client(mut socket: TcpStream, shared: Arc<Mutex<State>>) -> io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    let ClientMessage::Hello { name, profile } = protocol::read_client(&mut socket)? else {
        return Err(io::Error::new(ErrorKind::InvalidData, "expected Hello"));
    };
    if name.is_empty() || name.chars().any(char::is_control) || profile == 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid player name",
        ));
    }
    socket.set_read_timeout(None)?;
    let (sender, receiver) = mpsc::sync_channel(OUTBOUND_CAPACITY);
    let (id, seed, position, inventory) = {
        let mut state = shared.lock().unwrap();
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
        let inventory = state.inventory_store.load(profile)?;
        let id = state.next_id;
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or_else(|| io::Error::other("player ID exhausted"))?;
        let position = spawn_position(&mut state.world)?;
        let center = world_to_chunk(0, position[1] as i32, 0).0;
        state.clients.insert(
            id,
            Client {
                profile,
                inventory: inventory.clone(),
                last_drops_revision: u64::MAX,
                last_drop_anchor: [i32::MAX; 3],
                sender: sender.clone(),
                socket: socket.try_clone()?,
                sent: HashSet::new(),
                center,
                radius: DEFAULT_VIEW,
                position,
                last_move: Instant::now(),
                last_seq: 0,
            },
        );
        (id, state.seed, position, inventory)
    };
    sender
        .try_send(ServerMessage::Welcome { id, seed })
        .map_err(|_| io::Error::other("outbound queue closed"))?;
    sender
        .try_send(ServerMessage::Position {
            ack_seq: 0,
            x: position[0],
            y: position[1],
            z: position[2],
        })
        .map_err(|_| io::Error::other("outbound queue closed"))?;
    sender
        .try_send(ServerMessage::ViewDistance {
            radius: DEFAULT_VIEW,
        })
        .map_err(|_| io::Error::other("outbound queue closed"))?;
    sender
        .try_send(ServerMessage::Inventory {
            revision: inventory.revision,
            slots: inventory.slots,
        })
        .map_err(|_| io::Error::other("outbound queue closed"))?;
    let mut write_socket = socket.try_clone()?;
    write_socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    let writer = thread::spawn(move || {
        for message in receiver {
            if protocol::write_server(&mut write_socket, &message).is_err() {
                let _ = write_socket.shutdown(Shutdown::Both);
                break;
            }
        }
    });
    let stream_state = Arc::clone(&shared);
    let streamer = thread::spawn(move || {
        let mut ticks = 0u64;
        let mut total = Duration::ZERO;
        let mut peak = Duration::ZERO;
        let mut report_at = Instant::now();
        loop {
            thread::sleep(STREAM_INTERVAL);
            let started = Instant::now();
            let active = {
                let mut state = stream_state.lock().unwrap();
                stream_one(&mut state, id)
            };
            let elapsed = started.elapsed();
            ticks += 1;
            total += elapsed;
            peak = peak.max(elapsed);
            if report_at.elapsed() >= Duration::from_secs(5) {
                eprintln!(
                    "client {id}: {ticks} stream ticks, avg {:.2} ms, max {:.2} ms",
                    total.as_secs_f64() * 1000.0 / ticks as f64,
                    peak.as_secs_f64() * 1000.0
                );
                ticks = 0;
                total = Duration::ZERO;
                peak = Duration::ZERO;
                report_at = Instant::now();
            }
            if !active {
                break;
            }
        }
    });

    let result = loop {
        match protocol::read_client(&mut socket) {
            Ok(message) => {
                let mut state = shared.lock().unwrap();
                if let Err(error) = handle_message(&mut state, id, message) {
                    break Err(error);
                }
            }
            Err(error) => break Err(error),
        }
    };
    {
        let mut state = shared.lock().unwrap();
        state.clients.remove(&id);
    }
    let _ = socket.shutdown(Shutdown::Both);
    let _ = streamer.join();
    drop(sender);
    let _ = writer.join();
    match result {
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::BrokenPipe
            ) =>
        {
            Ok(())
        }
        other => other,
    }
}

fn spawn_position(world: &mut World) -> io::Result<[f32; 3]> {
    const HEADROOM: i32 = 32;
    let ceiling = MAX_TERRAIN_HEIGHT + HEADROOM;
    if world.get_block(0, ceiling, 0)? != AIR {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "spawn terrain exceeds scan ceiling",
        ));
    }
    for y in (0..ceiling).rev() {
        if world.get_block(0, y, 0)? != AIR {
            let position = [0.5, (y + 1) as f32, 0.5];
            if !collides(world, position)? {
                return Ok(position);
            }
        }
    }
    Err(io::Error::new(
        ErrorKind::InvalidData,
        "no safe spawn at world origin",
    ))
}

fn stream_one(state: &mut State, id: u64) -> bool {
    if state.drops.has_expired() {
        let before_expiry = state.drops.clone();
        if state.drops.expire()
            && let Err(error) = state.drops.save()
        {
            state.drops = before_expiry;
            eprintln!("expire drops: {error}");
            if let Some(client) = state.clients.get(&id) {
                let _ = client.socket.shutdown(Shutdown::Both);
            }
            return false;
        }
    }
    if let Err(error) = collect_nearby(state, id) {
        eprintln!("collect drops: {error}");
        if let Some(client) = state.clients.get(&id) {
            let _ = client.socket.shutdown(Shutdown::Both);
        }
        return false;
    }
    let Some(client) = state.clients.get_mut(&id) else {
        return false;
    };
    let anchor = client
        .position
        .map(|coordinate| (coordinate / 4.0).floor() as i32);
    let drop_revision = state.drops.revision();
    if client.last_drops_revision != drop_revision || client.last_drop_anchor != anchor {
        let items = state.drops.nearby(client.position);
        if !client.enqueue(ServerMessage::Drops {
            revision: drop_revision,
            items,
        }) {
            return false;
        }
        client.last_drops_revision = drop_revision;
        client.last_drop_anchor = anchor;
    }
    let center = client.center;
    let radius = client.radius as i32;
    client.sent.retain(|key| {
        (key.x as i64 - center.x as i64).abs() <= radius as i64
            && (key.y as i64 - center.y as i64).abs() <= 1
            && (key.z as i64 - center.z as i64).abs() <= radius as i64
    });
    let mut next = None;
    // Increasing distance lets nearby chunks appear first. The bounded view makes this cheap.
    'search: for distance in 0..=(radius * 2 + 1) {
        for y in -1i32..=1 {
            for z in -radius..=radius {
                for x in -radius..=radius {
                    if x.abs() + y.abs() + z.abs() != distance {
                        continue;
                    }
                    let Some(kx) = center.x.checked_add(x) else {
                        continue;
                    };
                    let Some(ky) = center.y.checked_add(y) else {
                        continue;
                    };
                    let Some(kz) = center.z.checked_add(z) else {
                        continue;
                    };
                    let key = ChunkKey {
                        x: kx,
                        y: ky,
                        z: kz,
                    };
                    if !client.sent.contains(&key) {
                        next = Some(key);
                        break 'search;
                    }
                }
            }
        }
    }
    let Some(key) = next else {
        return true;
    };
    match state.world.get_chunk(key) {
        Ok(chunk) => {
            if client.enqueue(ServerMessage::Chunk(chunk)) {
                client.sent.insert(key);
                true
            } else {
                false
            }
        }
        Err(error) => {
            eprintln!("stream chunk {key:?}: {error}");
            let _ = client.socket.shutdown(Shutdown::Both);
            false
        }
    }
}

fn collect_nearby(state: &mut State, id: u64) -> io::Result<()> {
    let Some(client) = state.clients.get(&id) else {
        return Ok(());
    };
    let (profile, position, mut updated) =
        (client.profile, client.position, client.inventory.clone());
    let mut taken = Vec::new();
    for item in state.drops.pickup_candidates(position) {
        let remaining = updated.insert(item.block, item.count);
        if remaining != item.count {
            taken.push(crate::protocol::DroppedItem {
                count: item.count - remaining,
                ..item
            });
        }
    }
    if taken.is_empty() {
        return Ok(());
    }
    let before_drops = state.drops.clone();
    for item in &taken {
        state.drops.take(item.id, item.count);
    }
    if let Err(error) = state.drops.save() {
        state.drops = before_drops;
        return Err(error);
    }
    if let Err(error) = state.inventory_store.save(profile, &updated) {
        state.drops = before_drops;
        state.drops.save()?;
        return Err(error);
    }
    if let Some(client) = state.clients.get_mut(&id) {
        client.inventory = updated;
        client.enqueue(ServerMessage::Pickups { items: taken });
        client.enqueue(ServerMessage::Inventory {
            revision: client.inventory.revision,
            slots: client.inventory.slots,
        });
    }
    Ok(())
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
        ClientMessage::Move { seq, dx, dy, dz } => move_player(state, id, seq, [dx, dy, dz]),
        ClientMessage::Edit {
            x,
            y,
            z,
            block,
            slot,
        } => edit_block(state, id, x, y, z, block, slot),
        ClientMessage::InventoryMove { from, to, count } => move_stack(state, id, from, to, count),
        ClientMessage::DropStack { slot, count } => drop_stack(state, id, slot, count),
    }
}

fn move_stack(state: &mut State, id: u64, from: u8, to: u8, count: u16) -> io::Result<()> {
    let Some(client) = state.clients.get(&id) else {
        return Ok(());
    };
    let mut next = client.inventory.clone();
    if !next.transfer(from, to, count) {
        return Ok(());
    }
    state.inventory_store.save(client.profile, &next)?;
    if let Some(client) = state.clients.get_mut(&id) {
        client.inventory = next;
        client.enqueue(ServerMessage::Inventory {
            revision: client.inventory.revision,
            slots: client.inventory.slots,
        });
    }
    Ok(())
}

fn drop_stack(state: &mut State, id: u64, slot: u8, count: u16) -> io::Result<()> {
    let Some(client) = state.clients.get(&id) else {
        return Ok(());
    };
    let Some(stack) = client.inventory.slots.get(slot as usize).copied().flatten() else {
        return Ok(());
    };
    if count == 0 || count > stack.count {
        return Ok(());
    }
    let mut next = client.inventory.clone();
    next.slots[slot as usize] = (count < stack.count).then_some(crate::inventory::Stack {
        count: stack.count - count,
        ..stack
    });
    next.revision = next.revision.wrapping_add(1);
    state.inventory_store.save(client.profile, &next)?;
    let position = [
        client.position[0],
        client.position[1] + 0.8,
        client.position[2],
    ];
    let before_drops = state.drops.clone();
    state
        .drops
        .spawn(position, stack.block, count, Duration::from_millis(1500));
    if let Err(error) = state.drops.save() {
        state.drops = before_drops;
        state
            .inventory_store
            .save(client.profile, &client.inventory)?;
        return Err(error);
    }
    if let Some(client) = state.clients.get_mut(&id) {
        client.inventory = next;
        client.enqueue(ServerMessage::Inventory {
            revision: client.inventory.revision,
            slots: client.inventory.slots,
        });
    }
    Ok(())
}

fn move_player(state: &mut State, id: u64, seq: u64, delta: [f32; 3]) -> io::Result<()> {
    let Some(client) = state.clients.get_mut(&id) else {
        return Ok(());
    };
    if seq <= client.last_seq {
        return Ok(());
    }
    client.last_seq = seq;
    let now = Instant::now();
    let elapsed = now.duration_since(client.last_move).as_secs_f32().min(0.25);
    client.last_move = now;
    let length_sq = delta.iter().map(|v| v * v).sum::<f32>();
    if length_sq > (PLAYER_SPEED * elapsed + 0.2).powi(2) {
        let [x, y, z] = client.position;
        client.enqueue(ServerMessage::Position {
            ack_seq: seq,
            x,
            y,
            z,
        });
        return Ok(());
    }
    client.position = resolve_movement(&mut state.world, client.position, delta)?;
    let [x, y, z] = client.position;
    client.center = world_to_chunk(x.floor() as i32, y.floor() as i32, z.floor() as i32).0;
    client.enqueue(ServerMessage::Position {
        ack_seq: seq,
        x,
        y,
        z,
    });
    Ok(())
}

fn resolve_movement(
    world: &mut World,
    mut position: [f32; 3],
    delta: [f32; 3],
) -> io::Result<[f32; 3]> {
    // Resolve in short axis-aligned steps so even a delayed input cannot tunnel
    // through a one-block wall.
    for axis in [0, 2, 1] {
        let steps = (delta[axis].abs() / 0.25).ceil().max(1.0) as usize;
        let step = delta[axis] / steps as f32;
        for _ in 0..steps {
            let mut candidate = position;
            candidate[axis] += step;
            if candidate.iter().any(|v| v.abs() >= 1_000_000.0) || collides(world, candidate)? {
                break;
            }
            position = candidate;
        }
    }
    Ok(position)
}

fn collides(world: &mut World, feet: [f32; 3]) -> io::Result<bool> {
    for x in [feet[0] - 0.3, feet[0] + 0.3] {
        for y in [feet[1] + 0.05, feet[1] + 0.9, feet[1] + 1.75] {
            for z in [feet[2] - 0.3, feet[2] + 0.3] {
                if world.get_block(x.floor() as i32, y.floor() as i32, z.floor() as i32)? != 0 {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn edit_block(
    state: &mut State,
    id: u64,
    x: i32,
    y: i32,
    z: i32,
    block: u8,
    slot: u8,
) -> io::Result<()> {
    let Some(client) = state.clients.get(&id) else {
        return Ok(());
    };
    if block > MAX_BLOCK {
        client.enqueue(ServerMessage::EditRejected {
            reason: "unknown block type".into(),
        });
        return Ok(());
    }
    if y <= BEDROCK_Y {
        client.enqueue(ServerMessage::EditRejected {
            reason: "world bottom is immutable".into(),
        });
        return Ok(());
    }
    let [px, py, pz] = client.position;
    let distance_sq = (x as f32 + 0.5 - px).powi(2)
        + (y as f32 + 0.5 - (py + 1.6)).powi(2)
        + (z as f32 + 0.5 - pz).powi(2);
    if distance_sq > EDIT_REACH * EDIT_REACH || !client.interested(world_to_chunk(x, y, z).0) {
        client.enqueue(ServerMessage::EditRejected {
            reason: "block out of reach".into(),
        });
        return Ok(());
    }
    let previous = state.world.get_block(x, y, z)?;
    if previous == block {
        return Ok(());
    }
    let (key, version) = if block != AIR {
        if previous != AIR {
            client.enqueue(ServerMessage::EditRejected {
                reason: "replace only air blocks".into(),
            });
            return Ok(());
        }
        if state
            .clients
            .values()
            .any(|other| block_intersects_player([x, y, z], other.position))
        {
            client.enqueue(ServerMessage::EditRejected {
                reason: "block overlaps a player".into(),
            });
            return Ok(());
        }
        if slot as usize >= crate::inventory::HOTBAR_SLOTS
            || client.inventory.slots[slot as usize].is_none_or(|stack| stack.block != block)
        {
            client.enqueue(ServerMessage::EditRejected {
                reason: "selected stack is empty".into(),
            });
            return Ok(());
        }
        let mut next = client.inventory.clone();
        next.consume(slot, block);
        state.inventory_store.save(client.profile, &next)?;
        let result = match state.world.edit(x, y, z, block) {
            Ok(result) => result,
            Err(error) => {
                // Refund if the world write fails; a saved inventory is never spent silently.
                state
                    .inventory_store
                    .save(client.profile, &client.inventory)?;
                return Err(error);
            }
        };
        if let Some(client) = state.clients.get_mut(&id) {
            client.inventory = next;
            client.enqueue(ServerMessage::Inventory {
                revision: client.inventory.revision,
                slots: client.inventory.slots,
            });
        }
        result
    } else {
        let result = state.world.edit(x, y, z, block)?;
        if previous != AIR {
            let before_drops = state.drops.clone();
            state.drops.spawn(
                [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5],
                previous,
                1,
                Duration::from_millis(250),
            );
            if let Err(error) = state.drops.save() {
                state.drops = before_drops;
                state.world.edit(x, y, z, previous)?;
                return Err(error);
            }
        }
        result
    };
    let (_, local) = world_to_chunk(x, y, z);
    let delta = ServerMessage::Delta {
        key,
        version,
        x: local[0] as u8,
        y: local[1] as u8,
        z: local[2] as u8,
        block,
    };
    // Each connection's FIFO writer preserves snapshot -> edit order.
    for client in state.clients.values() {
        if client.sent.contains(&key) {
            client.enqueue(delta.clone());
        }
    }
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

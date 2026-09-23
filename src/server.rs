//! Authoritative TCP server. A single world lock orders snapshots and edits for
//! every client. Cold chunk generation and durable edits currently serialize on
//! that lock; the per-client stream rate is bounded, and stream tick timings are
//! reported so this limit is visible under the 16-player target load.
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::world::{AIR, ChunkKey, MAX_TERRAIN_HEIGHT, STONE, World, world_to_chunk};
use std::collections::{HashMap, HashSet};
use std::io::{self, ErrorKind};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const MAX_CLIENTS: usize = 16;
const OUTBOUND_CAPACITY: usize = 128;
const DEFAULT_VIEW: u8 = 3;
const MAX_VIEW: u8 = 6;
const STREAM_INTERVAL: Duration = Duration::from_millis(20);
const PLAYER_SPEED: f32 = 10.0;
const EDIT_REACH: f32 = 8.0;

struct Client {
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
    seed: u64,
    clients: HashMap<u64, Client>,
    next_id: u64,
}

pub fn run_server(addr: &str, seed: u64, save_dir: PathBuf) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    let connections = Arc::new(AtomicUsize::new(0));
    let state = Arc::new(Mutex::new(State {
        world: World::new(seed, save_dir)?,
        seed,
        clients: HashMap::new(),
        next_id: 1,
    }));
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
    let ClientMessage::Hello { name } = protocol::read_client(&mut socket)? else {
        return Err(io::Error::new(ErrorKind::InvalidData, "expected Hello"));
    };
    if name.is_empty() || name.chars().any(char::is_control) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid player name",
        ));
    }
    socket.set_read_timeout(None)?;
    let (sender, receiver) = mpsc::sync_channel(OUTBOUND_CAPACITY);
    let (id, seed, position) = {
        let mut state = shared.lock().unwrap();
        if state.clients.len() >= MAX_CLIENTS {
            return Err(io::Error::new(ErrorKind::ConnectionRefused, "server full"));
        }
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
        (id, state.seed, position)
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
    let Some(client) = state.clients.get_mut(&id) else {
        return false;
    };
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
                client.radius = radius.clamp(1, MAX_VIEW);
            }
            Ok(())
        }
        ClientMessage::Resync { key } => {
            if let Some(client) = state.clients.get_mut(&id) {
                if client.interested(key) {
                    client.sent.remove(&key);
                }
            }
            Ok(())
        }
        ClientMessage::Move { seq, dx, dy, dz } => move_player(state, id, seq, [dx, dy, dz]),
        ClientMessage::Edit { x, y, z, block } => edit_block(state, id, x, y, z, block),
    }
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

fn edit_block(state: &mut State, id: u64, x: i32, y: i32, z: i32, block: u8) -> io::Result<()> {
    let Some(client) = state.clients.get(&id) else {
        return Ok(());
    };
    if block > STONE {
        client.enqueue(ServerMessage::EditRejected {
            reason: "unknown block type".into(),
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
    if state.world.get_block(x, y, z)? == block {
        return Ok(());
    }
    let (key, version) = state.world.edit(x, y, z, block)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::net::TcpListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn delayed_movement_cannot_cross_a_block() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("bloxgloom-server-{}-{stamp}", std::process::id()));
        let mut world = World::new(1, path.clone()).unwrap();
        world.edit(1, 40, 0, STONE).unwrap();
        let position = resolve_movement(&mut world, [0.5, 40.0, 0.5], [2.0, 0.0, 0.0]).unwrap();
        assert!(
            position[0] < 1.0,
            "player crossed a one-block wall: {position:?}"
        );
        drop(world);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn spawn_is_above_terrain_with_player_headroom() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        for (index, seed) in [0, 1, 7, 0xB10C_6100, u64::MAX].into_iter().enumerate() {
            let path = std::env::temp_dir().join(format!(
                "bloxgloom-spawn-{}-{stamp}-{index}",
                std::process::id()
            ));
            let mut world = World::new(seed, path.clone()).unwrap();
            let position = spawn_position(&mut world).unwrap();
            let feet_y = position[1] as i32;
            assert_eq!(world.get_block(0, feet_y, 0).unwrap(), AIR);
            assert_eq!(world.get_block(0, feet_y + 1, 0).unwrap(), AIR);
            assert_ne!(world.get_block(0, feet_y - 1, 0).unwrap(), AIR);
            assert!(!collides(&mut world, position).unwrap());
            drop(world);
            fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn two_clients_share_edit_and_resync_stays_ordered() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("bloxgloom-wire-{}-{stamp}", std::process::id()));
        let shared = Arc::new(Mutex::new(State {
            world: World::new(7, path.clone()).unwrap(),
            seed: 7,
            clients: HashMap::new(),
            next_id: 1,
        }));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_state = Arc::clone(&shared);
        let server = thread::spawn(move || {
            let mut workers = Vec::new();
            for _ in 0..2 {
                let (socket, _) = listener.accept().unwrap();
                let state = Arc::clone(&server_state);
                workers.push(thread::spawn(move || serve_client(socket, state).unwrap()));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        let mut socket = TcpStream::connect(address).unwrap();
        let mut peer = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        protocol::write_client(
            &mut socket,
            &ClientMessage::Hello {
                name: "Tester".into(),
            },
        )
        .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "Peer".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            protocol::read_server(&mut socket).unwrap(),
            ServerMessage::Welcome { seed: 7, .. }
        ));
        let feet_y = match protocol::read_server(&mut socket).unwrap() {
            ServerMessage::Position { ack_seq: 0, y, .. } => y as i32,
            other => panic!("expected initial position, got {other:?}"),
        };
        assert!(matches!(
            protocol::read_server(&mut peer).unwrap(),
            ServerMessage::Welcome { seed: 7, .. }
        ));
        assert!(matches!(
            protocol::read_server(&mut peer).unwrap(),
            ServerMessage::Position { ack_seq: 0, .. }
        ));
        let block_y = feet_y - 1;
        let (key, local) = world_to_chunk(0, block_y, 0);
        let version = (0..200)
            .find_map(|_| match protocol::read_server(&mut socket).unwrap() {
                ServerMessage::Chunk(chunk) if chunk.key == key => {
                    assert_ne!(chunk.blocks[crate::world::Chunk::index(local).unwrap()], 0);
                    Some(chunk.version)
                }
                _ => None,
            })
            .expect("target chunk snapshot");
        let peer_version = (0..200)
            .find_map(|_| match protocol::read_server(&mut peer).unwrap() {
                ServerMessage::Chunk(chunk) if chunk.key == key => Some(chunk.version),
                _ => None,
            })
            .expect("peer target chunk snapshot");
        assert_eq!(peer_version, version);
        protocol::write_client(
            &mut socket,
            &ClientMessage::Edit {
                x: 0,
                y: block_y,
                z: 0,
                block: 0,
            },
        )
        .unwrap();
        let new_version = (0..200)
            .find_map(|_| match protocol::read_server(&mut socket).unwrap() {
                ServerMessage::Delta {
                    key: got,
                    version,
                    x,
                    y,
                    z,
                    block,
                } if got == key => {
                    assert_eq!([x as usize, y as usize, z as usize], local);
                    assert_eq!(block, 0);
                    Some(version)
                }
                _ => None,
            })
            .expect("durable edit delta");
        assert_eq!(new_version, version + 1);
        let peer_delta = (0..200)
            .find_map(|_| match protocol::read_server(&mut peer).unwrap() {
                ServerMessage::Delta {
                    key: got,
                    version,
                    block,
                    ..
                } if got == key => {
                    assert_eq!(block, 0);
                    Some(version)
                }
                _ => None,
            })
            .expect("peer edit delta");
        assert_eq!(peer_delta, new_version);
        protocol::write_client(&mut peer, &ClientMessage::Resync { key }).unwrap();
        let refreshed = (0..200)
            .find_map(|_| match protocol::read_server(&mut peer).unwrap() {
                ServerMessage::Chunk(chunk) if chunk.key == key && chunk.version == new_version => {
                    Some(chunk)
                }
                _ => None,
            })
            .expect("resync snapshot");
        assert_eq!(
            refreshed.blocks[crate::world::Chunk::index(local).unwrap()],
            0
        );
        drop(socket);
        drop(peer);
        server.join().unwrap();
        drop(shared);
        let restarted = Arc::new(Mutex::new(State {
            world: World::new(7, path.clone()).unwrap(),
            seed: 7,
            clients: HashMap::new(),
            next_id: 1,
        }));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_state = Arc::clone(&restarted);
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            serve_client(socket, server_state).unwrap();
        });
        let mut reconnect = TcpStream::connect(address).unwrap();
        reconnect
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        protocol::write_client(
            &mut reconnect,
            &ClientMessage::Hello {
                name: "Returning".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            protocol::read_server(&mut reconnect).unwrap(),
            ServerMessage::Welcome { seed: 7, .. }
        ));
        assert!(matches!(
            protocol::read_server(&mut reconnect).unwrap(),
            ServerMessage::Position { ack_seq: 0, .. }
        ));
        let persisted = (0..200)
            .find_map(|_| match protocol::read_server(&mut reconnect).unwrap() {
                ServerMessage::Chunk(chunk) if chunk.key == key => Some(chunk),
                _ => None,
            })
            .expect("persisted chunk after restart");
        assert_eq!(persisted.version, new_version);
        assert_eq!(
            persisted.blocks[crate::world::Chunk::index(local).unwrap()],
            0
        );
        drop(reconnect);
        server.join().unwrap();
        drop(restarted);
        fs::remove_dir_all(path).unwrap();
    }
}

//! Blocking load-generator peers. Production server sockets still use the
//! fixed-thread nonblocking reactor; these threads exist only in the harness.

use crate::content::{ContentManifest, catalog};
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::world::ChunkKey;
use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "client/world.rs"]
mod world;
use world::WorldProbe;

#[derive(Clone, Copy)]
pub(super) struct Ready {
    pub epoch: u64,
    pub position: [f32; 3],
}

#[derive(Default)]
pub(super) struct ClientStats {
    pub chunks: u64,
    pub deltas: u64,
    pub positions: u64,
    pub drops: u64,
    pub pickups: u64,
    pub accepted_actions: u64,
    pub rejected_actions: u64,
    pub accepted_edits: u64,
    pub rejected_edits: u64,
    pub revision_gaps: u64,
    pub revision_regressions: u64,
    pub action_latencies: Vec<Duration>,
    pub last_position: Option<[f32; 3]>,
}

#[derive(Clone, Copy)]
enum ActionKind {
    Drop,
    Edit,
}

struct PendingAction {
    started: Instant,
    kind: ActionKind,
}

pub(super) struct ClientHandle {
    writer: Arc<Mutex<TcpStream>>,
    stopping: Arc<AtomicBool>,
    reader: Option<JoinHandle<io::Result<ClientStats>>>,
    ready: Receiver<io::Result<Ready>>,
    pub epoch: u64,
    pub profile: u128,
    pub move_seq: u64,
    pub action_seq: u64,
    pending_actions: Arc<Mutex<HashMap<u128, PendingAction>>>,
    latest_position: Arc<Mutex<[f32; 3]>>,
}

impl ClientHandle {
    pub fn connect(address: SocketAddr, index: usize, profile: u128) -> io::Result<Self> {
        let socket = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
        socket.set_nodelay(true)?;
        socket.set_read_timeout(Some(Duration::from_secs(30)))?;
        socket.set_write_timeout(Some(Duration::from_secs(5)))?;
        let writer = Arc::new(Mutex::new(socket.try_clone()?));
        let stopping = Arc::new(AtomicBool::new(false));
        let pending_actions = Arc::new(Mutex::new(HashMap::new()));
        let latest_position = Arc::new(Mutex::new([0.0; 3]));
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let thread_writer = Arc::clone(&writer);
        let thread_stopping = Arc::clone(&stopping);
        let thread_pending = Arc::clone(&pending_actions);
        let thread_position = Arc::clone(&latest_position);
        let reader = thread::Builder::new()
            .name(format!("tcp-soak-client-{index}"))
            .spawn(move || {
                let mut socket = socket;
                let result = handshake(&mut socket, &thread_writer, profile, index);
                let initial_position = match result {
                    Ok(ready) => {
                        *thread_position
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = ready.position;
                        let _ = ready_tx.send(Ok(ready));
                        ready.position
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(io::Error::new(error.kind(), error.to_string())));
                        return Err(error);
                    }
                };
                socket.set_read_timeout(None)?;
                read_until_stop(
                    socket,
                    thread_writer,
                    thread_stopping,
                    thread_pending,
                    thread_position,
                    initial_position,
                )
            })?;
        Ok(Self {
            writer,
            stopping,
            reader: Some(reader),
            ready,
            epoch: 0,
            profile,
            move_seq: 0,
            action_seq: 0,
            pending_actions,
            latest_position,
        })
    }

    pub fn await_ready(&mut self) -> io::Result<Ready> {
        let ready = self
            .ready
            .recv_timeout(Duration::from_secs(60))
            .map_err(|error| {
                io::Error::new(
                    ErrorKind::TimedOut,
                    format!("client {} handshake: {error}", self.profile),
                )
            })??;
        self.epoch = ready.epoch;
        Ok(ready)
    }

    pub fn send(&self, message: &ClientMessage) -> io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        protocol::write_client(&mut *writer, message)
    }

    pub fn send_move(&mut self, dx: f32, dy: f32, dz: f32) -> io::Result<()> {
        self.move_seq = self
            .move_seq
            .checked_add(1)
            .ok_or_else(|| io::Error::other("movement sequence exhausted"))?;
        self.send(&ClientMessage::Move {
            seq: self.move_seq,
            dx,
            dy,
            dz,
        })
    }

    pub fn drop_one(&mut self) -> io::Result<()> {
        let action_id = self.next_action_id()?;
        self.send_action(
            action_id,
            ActionKind::Drop,
            ClientMessage::DropStack {
                action_id,
                slot: 0,
                count: 1,
            },
        )
    }

    pub fn edit_support_block(&mut self) -> io::Result<()> {
        let [x, y, z] = *self
            .latest_position
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let action_id = self.next_action_id()?;
        self.send_action(
            action_id,
            ActionKind::Edit,
            ClientMessage::Edit {
                action_id,
                x: x.floor() as i32,
                y: y.floor() as i32 - 1,
                z: z.floor() as i32,
                block: crate::world::AIR,
                slot: 0,
            },
        )
    }

    fn next_action_id(&mut self) -> io::Result<u128> {
        self.action_seq = self
            .action_seq
            .checked_add(1)
            .ok_or_else(|| io::Error::other("action sequence exhausted"))?;
        Ok(u128::from(self.epoch) << 64 | u128::from(self.action_seq))
    }

    fn send_action(
        &self,
        action_id: u128,
        kind: ActionKind,
        message: ClientMessage,
    ) -> io::Result<()> {
        self.pending_actions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                action_id,
                PendingAction {
                    started: Instant::now(),
                    kind,
                },
            );
        if let Err(error) = self.send(&message) {
            self.pending_actions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&action_id);
            return Err(error);
        }
        Ok(())
    }

    pub fn stop(mut self) -> io::Result<ClientStats> {
        self.stopping.store(true, Ordering::Release);
        let _ = self
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown(Shutdown::Both);
        let stats = self
            .reader
            .take()
            .expect("client reader exists")
            .join()
            .map_err(|_| io::Error::other("load-generator reader panicked"))??;
        let pending = self
            .pending_actions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        if pending != 0 {
            return Err(io::Error::other(format!(
                "{pending} TCP actions have no result"
            )));
        }
        Ok(stats)
    }
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Ok(writer) = self.writer.lock() {
            let _ = writer.shutdown(Shutdown::Both);
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn handshake(
    socket: &mut TcpStream,
    writer: &Arc<Mutex<TcpStream>>,
    profile: u128,
    index: usize,
) -> io::Result<Ready> {
    let fingerprint = catalog().fingerprint();
    protocol::write_client(
        &mut *socket,
        &ClientMessage::Hello {
            name: format!("soak-{index}"),
            profile,
            content_fingerprint: fingerprint,
        },
    )?;
    let expected = ContentManifest::from_catalog(catalog()).encode()?;
    let mut manifest = Vec::new();
    while manifest.len() < expected.len() {
        let ServerMessage::ContentManifestPart {
            fingerprint: part_fingerprint,
            total_len,
            offset,
            bytes,
        } = protocol::read_server(&mut *socket)?
        else {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "expected manifest part",
            ));
        };
        if part_fingerprint != fingerprint
            || total_len as usize != expected.len()
            || offset as usize != manifest.len()
            || bytes.is_empty()
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "manifest identity/offset mismatch",
            ));
        }
        manifest.extend_from_slice(&bytes);
    }
    if manifest != expected {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "manifest content mismatch",
        ));
    }
    protocol::write_client(&mut *socket, &ClientMessage::ContentReady { fingerprint })?;
    let mut epoch = None;
    let mut position = None;
    let mut welcome = false;
    let mut view = false;
    let mut inventory = false;
    while !(welcome && epoch.is_some() && position.is_some() && view && inventory) {
        match protocol::read_server(&mut *socket)? {
            ServerMessage::Welcome { .. } => welcome = true,
            ServerMessage::ActionSession { epoch: value, .. } => epoch = Some(value),
            ServerMessage::Position { x, y, z, .. } => position = Some([x, y, z]),
            ServerMessage::ViewDistance { .. } => view = true,
            ServerMessage::Inventory { .. } => inventory = true,
            _ => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "terrain arrived before ordered join handshake",
                ));
            }
        }
    }
    let _ = writer;
    Ok(Ready {
        epoch: epoch.unwrap(),
        position: position.unwrap(),
    })
}

fn read_until_stop(
    mut socket: TcpStream,
    writer: Arc<Mutex<TcpStream>>,
    stopping: Arc<AtomicBool>,
    pending_actions: Arc<Mutex<HashMap<u128, PendingAction>>>,
    latest_position: Arc<Mutex<[f32; 3]>>,
    initial_position: [f32; 3],
) -> io::Result<ClientStats> {
    let mut stats = ClientStats {
        last_position: Some(initial_position),
        ..ClientStats::default()
    };
    let mut revisions: HashMap<ChunkKey, u64> = HashMap::new();
    let mut world = WorldProbe::default();
    loop {
        let message = match protocol::read_server(&mut socket) {
            Ok(message) => message,
            Err(_error) if stopping.load(Ordering::Acquire) => return Ok(stats),
            Err(error) => return Err(error),
        };
        match message {
            ServerMessage::Chunk(chunk) => {
                if revisions
                    .get(&chunk.key)
                    .is_some_and(|previous| chunk.version < *previous)
                {
                    stats.revision_regressions += 1;
                }
                revisions.insert(chunk.key, chunk.version);
                stats.chunks += 1;
            }
            ServerMessage::Delta { key, version, .. } => {
                if let Some(previous) = revisions.get(&key)
                    && version > previous.saturating_add(1)
                {
                    stats.revision_gaps += 1;
                    let mut writer = writer
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    protocol::write_client(&mut *writer, &ClientMessage::Resync { key })?;
                }
                if revisions
                    .get(&key)
                    .is_some_and(|previous| version < *previous)
                {
                    stats.revision_regressions += 1;
                }
                revisions.insert(key, version);
                stats.deltas += 1;
            }
            ServerMessage::WorldSnapshotStart(start) => {
                let update = world.start(start)?;
                stats.chunks += update.snapshots;
                stats.revision_regressions += update.regressions;
            }
            ServerMessage::EntitySnapshotPage(page) => {
                let update = world.page(page)?;
                stats.chunks += update.snapshots;
            }
            ServerMessage::WorldCommitPart(part) => {
                let update = world.commit(part)?;
                stats.deltas += update.block_changes;
                stats.revision_gaps += update.gaps;
                stats.revision_regressions += update.regressions;
                for key in update.resync {
                    let mut writer = writer
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    protocol::write_client(&mut *writer, &ClientMessage::Resync { key })?;
                }
            }
            ServerMessage::Position { x, y, z, .. } => {
                stats.positions += 1;
                stats.last_position = Some([x, y, z]);
                *latest_position
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = [x, y, z];
            }
            ServerMessage::Drops { .. } => stats.drops += 1,
            ServerMessage::Pickups { items } => stats.pickups += items.len() as u64,
            ServerMessage::ActionResult {
                action_id,
                accepted,
                ..
            } => {
                let pending = pending_actions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(&action_id)
                    .ok_or_else(|| {
                        io::Error::new(
                            ErrorKind::InvalidData,
                            "unrequested or duplicated action result",
                        )
                    })?;
                stats.action_latencies.push(pending.started.elapsed());
                if accepted {
                    stats.accepted_actions += 1;
                } else {
                    stats.rejected_actions += 1;
                }
                if matches!(pending.kind, ActionKind::Edit) {
                    if accepted {
                        stats.accepted_edits += 1;
                    } else {
                        stats.rejected_edits += 1;
                    }
                }
                let mut writer = writer
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                protocol::write_client(
                    &mut *writer,
                    &ClientMessage::ActionAck {
                        epoch: (action_id >> 64) as u64,
                        through_seq: action_id as u64,
                    },
                )?;
            }
            ServerMessage::ActionDeferred { .. }
            | ServerMessage::OwnedEntity { .. }
            | ServerMessage::Inventory { .. }
            | ServerMessage::ViewDistance { .. }
            | ServerMessage::Pong { .. } => {}
            other => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!("unexpected post-join message: {other:?}"),
                ));
            }
        }
    }
}

/// Extra client exercising reconnect without borrowing a core steady-state slot.
pub(super) fn reconnect_probe(address: SocketAddr, profile: u128, index: usize) -> io::Result<()> {
    let mut socket = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    socket.set_read_timeout(Some(Duration::from_secs(30)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    let writer = Arc::new(Mutex::new(socket.try_clone()?));
    let _ = handshake(&mut socket, &writer, profile, index)?;
    socket.shutdown(Shutdown::Both)
}

/// Completes the same ordered production handshake as a healthy peer, then
/// deliberately stops reading while requesting the widest interest radius.
pub(super) fn active_slow_peer(
    address: SocketAddr,
    profile: u128,
    index: usize,
) -> io::Result<TcpStream> {
    let mut socket = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    socket.set_read_timeout(Some(Duration::from_secs(30)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    let writer = Arc::new(Mutex::new(socket.try_clone()?));
    let _ = handshake(&mut socket, &writer, profile, index)?;
    protocol::write_client(&mut socket, &ClientMessage::SetView { radius: 6 })?;
    Ok(socket)
}

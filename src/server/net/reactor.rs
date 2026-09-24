//! Single-threaded nonblocking socket reactor.
//!
//! Every accepted socket is polled by one fixed reactor thread. Disk-backed
//! inventory loads are handed to a small bounded worker pool; no thread is
//! created per client. Reads and writes have per-connection budgets so one
//! busy peer cannot monopolize a reactor pass.

use super::{ContentHandshake, HELLO_TIMEOUT, JOIN_TIMEOUT};
use crate::inventory::{Inventory, InventoryStore};
use crate::protocol::{self, ClientMessage, MAX_FRAME};
use crate::server::outbound::{OutboundFrame, OutboundQueue, OutboundTelemetry};
use crate::server::{
    INPUT_CAPACITY, JoinReply, JoinResponse, MAX_CLIENTS, SimulationInput, State,
    run_simulation_ticks,
};
use polling::{Event as PollEvent, Events, Poller};
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const IO_POLL_TIMEOUT: Duration = Duration::from_millis(10);
const READ_BUDGET: usize = 64 * 1024;
const WRITE_BUDGET: usize = 64 * 1024;
const SOCKET_CHUNK: usize = 8 * 1024;
const MAX_MESSAGES_PER_POLL: usize = 32;
const ACCEPT_BUDGET: usize = 64;
const INVENTORY_WORKERS: usize = 4;
const MAX_PENDING_LEAVES: usize = MAX_CLIENTS;
const LISTENER_KEY: usize = 0;

#[cfg(test)]
pub(super) fn has_admission_capacity(connection_count: usize, pending_leave_count: usize) -> bool {
    has_admission_capacity_with_limit(
        connection_count,
        pending_leave_count,
        crate::server::DEFAULT_CLIENTS,
    )
}

fn has_admission_capacity_with_limit(
    connection_count: usize,
    pending_leave_count: usize,
    limit: usize,
) -> bool {
    connection_count.saturating_add(pending_leave_count) < limit
}

#[cfg(test)]
#[path = "reactor/tests.rs"]
mod tests;

struct InventoryLoadRequest {
    profile: u128,
    reply: SyncSender<io::Result<Inventory>>,
}

/// Bounded, fixed-size pool for the filesystem portion of the join handshake.
struct InventoryWorkers {
    sender: Option<SyncSender<InventoryLoadRequest>>,
    workers: Vec<JoinHandle<()>>,
}

impl InventoryWorkers {
    fn new(store: InventoryStore) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(MAX_CLIENTS);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(INVENTORY_WORKERS);
        for index in 0..INVENTORY_WORKERS {
            let receiver = Arc::clone(&receiver);
            let store = store.clone();
            let worker = thread::Builder::new()
                .name(format!("server-inventory-{index}"))
                .spawn(move || inventory_worker(store, receiver))?;
            workers.push(worker);
        }
        Ok(Self {
            sender: Some(sender),
            workers,
        })
    }

    fn request(&self, profile: u128) -> Result<Receiver<io::Result<Inventory>>, ()> {
        let (reply, receiver) = mpsc::sync_channel(1);
        let request = InventoryLoadRequest { profile, reply };
        match self
            .sender
            .as_ref()
            .expect("inventory pool is active")
            .try_send(request)
        {
            Ok(()) => Ok(receiver),
            Err(TrySendError::Full(_)) => Err(()),
            Err(TrySendError::Disconnected(_)) => Err(()),
        }
    }
}

fn inventory_worker(store: InventoryStore, receiver: Arc<Mutex<Receiver<InventoryLoadRequest>>>) {
    loop {
        let request = receiver
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv();
        let Ok(request) = request else {
            return;
        };
        let result = store.load(request.profile);
        let _ = request.reply.try_send(result);
    }
}

impl Drop for InventoryWorkers {
    fn drop(&mut self) {
        drop(self.sender.take());
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

struct PendingLeave {
    id: u64,
    sequence: u64,
}

#[derive(Clone, Copy, Default)]
struct Readiness {
    readable: bool,
    writable: bool,
    error: bool,
}

/// Runs the nonblocking reactor and state coordinator. All connection socket
/// I/O, framing, and queue draining happen on this thread.
pub(super) fn serve_listener(listener: TcpListener, state: State) -> io::Result<()> {
    let admission_limit = state.admission_limit;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let inventory_store = state.inventory_store.clone();
    let outbound = Arc::clone(&state.outbound);
    let content = ContentHandshake::from_catalog(state.world.catalog_arc())?;
    let workers = InventoryWorkers::new(inventory_store)?;
    let poller = Poller::new()?;
    // SAFETY: the listener remains alive until explicitly removed below.
    unsafe {
        poller.add(&listener, PollEvent::readable(LISTENER_KEY))?;
    }
    let (input_sender, input_receiver) = mpsc::sync_channel(INPUT_CAPACITY);
    let coordinator = thread::spawn(move || run_simulation_ticks(state, input_receiver));

    eprintln!("Bloxgloom server listening on {address}");
    let mut connections = Vec::with_capacity(admission_limit);
    let mut pending_leaves = VecDeque::with_capacity(MAX_PENDING_LEAVES);
    let mut next_connection_key = LISTENER_KEY + 1;
    let mut reactor_error = None;
    let mut events = Events::new();

    while !coordinator.is_finished() {
        events.clear();
        if let Err(error) = poller.wait(&mut events, Some(IO_POLL_TIMEOUT)) {
            reactor_error = Some(error);
            break;
        }

        let mut ready = std::collections::HashMap::with_capacity(events.len());
        for event in events.iter() {
            let entry = ready
                .entry(event.key)
                .or_insert_with(|| Readiness::default());
            entry.readable |= event.readable;
            entry.writable |= event.writable;
            entry.error |= event.is_err() == Some(true);
        }

        if ready.remove(&LISTENER_KEY).is_some() {
            for _ in 0..ACCEPT_BUDGET {
                match listener.accept() {
                    Ok((socket, _peer)) => {
                        if !has_admission_capacity_with_limit(
                            connections.len(),
                            pending_leaves.len(),
                            admission_limit,
                        ) {
                            let _ = socket.shutdown(Shutdown::Both);
                            continue;
                        }
                        if let Err(error) = socket.set_nodelay(true) {
                            eprintln!("client socket: {error}");
                            let _ = socket.shutdown(Shutdown::Both);
                            continue;
                        }
                        if let Err(error) = socket.set_nonblocking(true) {
                            eprintln!("client nonblocking mode: {error}");
                            let _ = socket.shutdown(Shutdown::Both);
                            continue;
                        }
                        if next_connection_key == usize::MAX {
                            let _ = socket.shutdown(Shutdown::Both);
                            reactor_error =
                                Some(io::Error::other("connection key space exhausted"));
                            break;
                        }
                        let key = next_connection_key;
                        next_connection_key += 1;
                        // SAFETY: every registered stream is removed before its connection drops.
                        if let Err(error) = unsafe { poller.add(&socket, PollEvent::readable(key)) }
                        {
                            eprintln!("register client socket: {error}");
                            let _ = socket.shutdown(Shutdown::Both);
                            continue;
                        }
                        connections.push(Connection::new(socket, Arc::clone(&outbound), key));
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == ErrorKind::Interrupted => break,
                    Err(error) => {
                        reactor_error = Some(error);
                        break;
                    }
                }
            }
            if let Err(error) = poller.modify(&listener, PollEvent::readable(LISTENER_KEY)) {
                reactor_error = Some(error);
            }
        }

        if reactor_error.is_some() {
            break;
        }

        retry_leaves(&input_sender, &mut pending_leaves);

        let now = Instant::now();
        let mut index = 0;
        while index < connections.len() {
            let readiness = ready
                .get(&connections[index].poll_key)
                .copied()
                .unwrap_or_default();
            let result = connections[index].poll(
                now,
                &content,
                &workers,
                &input_sender,
                &mut pending_leaves,
                readiness,
            );
            match result {
                Ok(()) => {}
                Err(error) => {
                    if !matches!(
                        error.kind(),
                        ErrorKind::UnexpectedEof
                            | ErrorKind::ConnectionReset
                            | ErrorKind::BrokenPipe
                    ) {
                        eprintln!("client: {error}");
                    }
                    connections[index].disconnect(&mut pending_leaves);
                }
            }

            let rearm = readiness.readable || readiness.writable || readiness.error;
            if connections[index].update_interest(&poller, rearm).is_err() {
                connections[index].disconnect(&mut pending_leaves);
            }

            if connections[index].should_remove() {
                let connection = connections.swap_remove(index);
                if let Err(error) = poller.delete(&connection.socket) {
                    eprintln!("unregister client socket: {error}");
                }
                drop(connection);
            } else {
                index += 1;
            }
        }
    }

    for connection in &connections {
        if let Err(error) = poller.delete(&connection.socket) {
            eprintln!("unregister client socket: {error}");
        }
        let _ = connection.socket.shutdown(Shutdown::Both);
    }
    connections.clear();
    let _ = poller.delete(&listener);
    // Best effort on listener failure; normal peer leaves are retried in FIFO
    // order until the bounded coordinator input queue accepts them.
    retry_leaves(&input_sender, &mut pending_leaves);
    drop(input_sender);
    drop(workers);
    let coordinator_result = coordinator
        .join()
        .map_err(|_| io::Error::other("simulation coordinator panicked"))?;
    match reactor_error {
        Some(error) => Err(error),
        None => coordinator_result,
    }
}

fn retry_leaves(input: &SyncSender<SimulationInput>, pending: &mut VecDeque<PendingLeave>) -> bool {
    let attempts = pending.len();
    let mut progress = false;
    for _ in 0..attempts {
        let Some(leave) = pending.pop_front() else {
            break;
        };
        match input.try_send(SimulationInput::Leave {
            id: leave.id,
            sequence: leave.sequence,
        }) {
            Ok(()) => progress = true,
            Err(TrySendError::Full(SimulationInput::Leave { id, sequence })) => {
                pending.push_back(PendingLeave { id, sequence });
            }
            Err(TrySendError::Disconnected(_)) => return true,
            Err(TrySendError::Full(_)) => unreachable!("leave retry only sends Leave"),
        }
    }
    progress
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    AwaitHello,
    SendingManifest,
    AwaitContentReady,
    ReadyToLoadInventory,
    LoadingInventory,
    ReadyToJoin,
    AwaitJoin,
    Active,
    Closed,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PendingWriteKind {
    Manifest,
    Outbound,
}

struct PendingWrite {
    bytes: Arc<[u8]>,
    offset: usize,
    kind: PendingWriteKind,
    reservation: Option<OutboundFrame>,
}

impl PendingWrite {
    fn manifest(bytes: Arc<[u8]>) -> Self {
        Self {
            bytes,
            offset: 0,
            kind: PendingWriteKind::Manifest,
            reservation: None,
        }
    }

    fn outbound(bytes: Vec<u8>, reservation: OutboundFrame) -> Self {
        Self {
            bytes: Arc::from(bytes),
            offset: 0,
            kind: PendingWriteKind::Outbound,
            reservation: Some(reservation),
        }
    }
}

struct Connection {
    socket: TcpStream,
    poll_key: usize,
    read_interest: bool,
    write_interest: bool,
    phase: Phase,
    deadline: Instant,
    profile: Option<u128>,
    sequence: u64,
    player_id: Option<u64>,
    peer_closed: bool,
    input_buffer: Vec<u8>,
    manifest_index: usize,
    pending_write: Option<PendingWrite>,
    inventory: Option<Inventory>,
    inventory_receiver: Option<Receiver<io::Result<Inventory>>>,
    join_receiver: Option<Receiver<JoinResponse>>,
    outbound: Arc<OutboundTelemetry>,
    outbound_sender: Option<OutboundQueue>,
    outbound_receiver: Option<Receiver<OutboundFrame>>,
}

impl Connection {
    fn new(socket: TcpStream, outbound: Arc<OutboundTelemetry>, poll_key: usize) -> Self {
        Self {
            socket,
            poll_key,
            read_interest: true,
            write_interest: false,
            phase: Phase::AwaitHello,
            deadline: Instant::now() + HELLO_TIMEOUT,
            profile: None,
            sequence: 0,
            player_id: None,
            peer_closed: false,
            input_buffer: Vec::with_capacity(SOCKET_CHUNK),
            manifest_index: 0,
            pending_write: None,
            inventory: None,
            inventory_receiver: None,
            join_receiver: None,
            outbound,
            outbound_sender: None,
            outbound_receiver: None,
        }
    }

    fn poll(
        &mut self,
        now: Instant,
        content: &ContentHandshake,
        workers: &InventoryWorkers,
        input: &SyncSender<SimulationInput>,
        pending_leaves: &mut VecDeque<PendingLeave>,
        readiness: Readiness,
    ) -> io::Result<()> {
        if self.phase == Phase::Closed {
            return Ok(());
        }
        if now >= self.deadline && self.phase != Phase::Active {
            if self.phase == Phase::AwaitJoin {
                self.peer_closed = true;
                let _ = self.socket.shutdown(Shutdown::Both);
            } else {
                return Err(io::Error::new(
                    ErrorKind::TimedOut,
                    "client handshake timed out",
                ));
            }
        }

        self.poll_async(now, workers, input, pending_leaves)?;
        if self.peer_closed {
            return Ok(());
        }
        if readiness.error {
            return Err(io::Error::new(
                ErrorKind::ConnectionReset,
                "poller reported a socket error",
            ));
        }
        if readiness.writable {
            self.poll_write()?;
        }
        if readiness.readable || (self.phase == Phase::Active && !self.input_buffer.is_empty()) {
            self.poll_read(now, content, input)?;
        }
        self.poll_async(now, workers, input, pending_leaves)?;
        self.prepare_write(content)?;
        Ok(())
    }

    fn update_interest(&mut self, poller: &Poller, force_rearm: bool) -> io::Result<()> {
        let waiting_for_join = matches!(
            self.phase,
            Phase::ReadyToLoadInventory
                | Phase::LoadingInventory
                | Phase::ReadyToJoin
                | Phase::AwaitJoin
        );
        let read_interest = !self.peer_closed && self.phase != Phase::Closed && !waiting_for_join;
        let write_interest = !self.peer_closed && self.pending_write.is_some();
        if force_rearm
            || read_interest != self.read_interest
            || write_interest != self.write_interest
        {
            poller.modify(
                &self.socket,
                PollEvent::new(self.poll_key, read_interest, write_interest),
            )?;
            self.read_interest = read_interest;
            self.write_interest = write_interest;
        }
        Ok(())
    }

    fn poll_async(
        &mut self,
        now: Instant,
        workers: &InventoryWorkers,
        input: &SyncSender<SimulationInput>,
        pending_leaves: &mut VecDeque<PendingLeave>,
    ) -> io::Result<bool> {
        let mut progress = false;
        match self.phase {
            Phase::ReadyToLoadInventory => {
                let profile = self.profile.expect("Hello establishes a profile");
                match workers.request(profile) {
                    Ok(receiver) => {
                        self.inventory_receiver = Some(receiver);
                        self.phase = Phase::LoadingInventory;
                        progress = true;
                    }
                    Err(()) => {}
                }
            }
            Phase::LoadingInventory => {
                match self
                    .inventory_receiver
                    .as_ref()
                    .expect("loading phase has a receiver")
                    .try_recv()
                {
                    Ok(Ok(inventory)) => {
                        self.inventory_receiver = None;
                        self.inventory = Some(inventory);
                        self.phase = Phase::ReadyToJoin;
                        progress = true;
                    }
                    Ok(Err(error)) => return Err(error),
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        self.phase = Phase::Closed;
                        return Err(io::Error::other("inventory worker stopped"));
                    }
                }
            }
            Phase::ReadyToJoin => {
                let profile = self.profile.expect("Hello establishes a profile");
                let inventory = self.inventory.take().expect("join phase has inventory");
                self.ensure_outbound();
                let sender = self.outbound_sender.as_ref().unwrap().clone();
                let (reply, receiver) = mpsc::sync_channel(1);
                match input.try_send(SimulationInput::Join {
                    profile,
                    inventory,
                    sender: sender.clone(),
                    socket: self.socket.try_clone()?,
                    reply,
                }) {
                    Ok(()) => {
                        self.join_receiver = Some(receiver);
                        self.phase = Phase::AwaitJoin;
                        self.deadline = now + JOIN_TIMEOUT;
                        progress = true;
                    }
                    Err(TrySendError::Full(SimulationInput::Join { inventory, .. })) => {
                        self.inventory = Some(inventory);
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        return Err(io::Error::new(
                            ErrorKind::BrokenPipe,
                            "simulation coordinator stopped",
                        ));
                    }
                    Err(TrySendError::Full(_)) => unreachable!("join retry only sends Join"),
                }
            }
            Phase::AwaitJoin => {
                match self
                    .join_receiver
                    .as_ref()
                    .expect("join phase has a receiver")
                    .try_recv()
                {
                    Ok(JoinResponse::RefreshInventory) => {
                        self.join_receiver = None;
                        if self.peer_closed {
                            self.phase = Phase::Closed;
                        } else {
                            self.phase = Phase::ReadyToLoadInventory;
                        }
                        progress = true;
                    }
                    Ok(JoinResponse::Completed(result)) => {
                        let JoinReply { id } = match *result {
                            Ok(reply) => reply,
                            Err(error) => {
                                self.phase = Phase::Closed;
                                return Err(error);
                            }
                        };
                        self.join_receiver = None;
                        self.player_id = Some(id);
                        if self.peer_closed {
                            self.queue_leave(pending_leaves);
                        } else {
                            self.phase = Phase::Active;
                            self.outbound_sender = None;
                        }
                        progress = true;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        self.phase = Phase::Closed;
                        return Err(io::Error::other("join coordinator stopped"));
                    }
                }
            }
            _ => {}
        }
        Ok(progress)
    }

    fn prepare_write(&mut self, content: &ContentHandshake) -> io::Result<()> {
        if self.pending_write.is_none() {
            match self.phase {
                Phase::SendingManifest => {
                    if let Some(frame) = content.manifest_frames.get(self.manifest_index) {
                        self.pending_write = Some(PendingWrite::manifest(Arc::clone(frame)));
                    } else {
                        self.phase = Phase::AwaitContentReady;
                        self.deadline = Instant::now() + HELLO_TIMEOUT;
                    }
                }
                Phase::Active => {
                    let receiver = self
                        .outbound_receiver
                        .as_ref()
                        .expect("active connection has an outbound receiver");
                    match receiver.try_recv() {
                        Ok(frame) => {
                            let mut bytes =
                                Vec::with_capacity(protocol::server_wire_len(frame.message()));
                            if let Err(error) = protocol::write_server_with_catalog(
                                &mut bytes,
                                frame.message(),
                                &content.catalog,
                            ) {
                                return Err(error);
                            }
                            self.pending_write = Some(PendingWrite::outbound(bytes, frame));
                        }
                        Err(TryRecvError::Empty) => {}
                        Err(TryRecvError::Disconnected) => {
                            return Err(io::Error::new(
                                ErrorKind::BrokenPipe,
                                "client outbound queue closed",
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn poll_write(&mut self) -> io::Result<()> {
        let Some(pending) = self.pending_write.as_mut() else {
            return Ok(());
        };
        let mut written = 0;
        while pending.offset < pending.bytes.len() && written < WRITE_BUDGET {
            let end = (pending.offset + WRITE_BUDGET - written).min(pending.bytes.len());
            match self.socket.write(&pending.bytes[pending.offset..end]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        ErrorKind::WriteZero,
                        "socket write returned zero",
                    ));
                }
                Ok(count) => {
                    pending.offset += count;
                    written += count;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }

        if self
            .pending_write
            .as_ref()
            .is_some_and(|pending| pending.offset == pending.bytes.len())
        {
            let Some(mut completed) = self.pending_write.take() else {
                unreachable!();
            };
            if completed.kind == PendingWriteKind::Manifest {
                self.manifest_index += 1;
            } else if let Some(frame) = completed.reservation.take() {
                frame.record_sent();
                drop(frame);
            }
        }
        Ok(())
    }

    fn poll_read(
        &mut self,
        now: Instant,
        content: &ContentHandshake,
        input: &SyncSender<SimulationInput>,
    ) -> io::Result<bool> {
        if matches!(
            self.phase,
            Phase::ReadyToLoadInventory
                | Phase::LoadingInventory
                | Phase::ReadyToJoin
                | Phase::AwaitJoin
        ) {
            // ContentReady is the gate. Bytes that arrived after it remain in
            // the bounded parser buffer until Join completes; commands are
            // never submitted against a player ID that has not been created.
            return Ok(false);
        }
        let mut progress = false;
        let mut bytes_read = 0;
        let mut messages = 0;
        loop {
            while messages < MAX_MESSAGES_PER_POLL {
                let Some(frame) = self.take_complete_frame()? else {
                    break;
                };
                let message = protocol::read_client_with_catalog(&frame[..], &content.catalog)?;
                self.handle_message(now, message, content, input)?;
                messages += 1;
                progress = true;
                if matches!(
                    self.phase,
                    Phase::ReadyToLoadInventory
                        | Phase::LoadingInventory
                        | Phase::ReadyToJoin
                        | Phase::AwaitJoin
                ) {
                    break;
                }
            }
            if messages >= MAX_MESSAGES_PER_POLL || bytes_read >= READ_BUDGET {
                break;
            }

            let mut buffer = [0u8; SOCKET_CHUNK];
            let amount = (READ_BUDGET - bytes_read).min(buffer.len());
            match self.socket.read(&mut buffer[..amount]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "client disconnected",
                    ));
                }
                Ok(count) => {
                    self.input_buffer.extend_from_slice(&buffer[..count]);
                    bytes_read += count;
                    progress = true;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(progress)
    }

    fn take_complete_frame(&mut self) -> io::Result<Option<Vec<u8>>> {
        if self.input_buffer.len() < 4 {
            return Ok(None);
        }
        let length = u32::from_le_bytes(
            self.input_buffer[..4]
                .try_into()
                .expect("four-byte frame header"),
        ) as usize;
        if !(2..=MAX_FRAME).contains(&length) {
            return Err(io::Error::new(ErrorKind::InvalidData, "invalid frame size"));
        }
        let frame_length = length + 4;
        if self.input_buffer.len() < frame_length {
            return Ok(None);
        }
        Ok(Some(self.input_buffer.drain(..frame_length).collect()))
    }

    fn handle_message(
        &mut self,
        now: Instant,
        message: ClientMessage,
        content: &ContentHandshake,
        input: &SyncSender<SimulationInput>,
    ) -> io::Result<()> {
        match (self.phase, message) {
            (
                Phase::AwaitHello,
                ClientMessage::Hello {
                    name,
                    profile,
                    content_fingerprint: _,
                },
            ) if !name.is_empty() && !name.chars().any(char::is_control) && profile != 0 => {
                self.profile = Some(profile);
                self.phase = Phase::SendingManifest;
                self.deadline = now + HELLO_TIMEOUT;
                Ok(())
            }
            (Phase::AwaitContentReady, ClientMessage::ContentReady { fingerprint })
                if fingerprint == content.fingerprint =>
            {
                self.phase = Phase::ReadyToLoadInventory;
                self.deadline = now + JOIN_TIMEOUT;
                Ok(())
            }
            (Phase::Active, ClientMessage::Hello { .. } | ClientMessage::ContentReady { .. }) => {
                Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "duplicate content handshake message",
                ))
            }
            (Phase::Active, message) => {
                let id = self.player_id.expect("active connection has a player ID");
                let sequence = self
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("client command sequence exhausted"))?;
                input
                    .try_send(SimulationInput::Command {
                        id,
                        sequence,
                        message,
                    })
                    .map_err(|_| {
                        io::Error::new(ErrorKind::WouldBlock, "server input queue unavailable")
                    })?;
                self.sequence = sequence;
                Ok(())
            }
            _ => Err(io::Error::new(
                ErrorKind::InvalidData,
                "unexpected message for connection state",
            )),
        }
    }

    fn disconnect(&mut self, pending_leaves: &mut VecDeque<PendingLeave>) {
        if self.peer_closed || self.phase == Phase::Closed {
            return;
        }
        self.peer_closed = true;
        let _ = self.socket.shutdown(Shutdown::Both);
        if self.phase == Phase::Active {
            self.queue_leave(pending_leaves);
        } else if self.phase != Phase::AwaitJoin {
            self.phase = Phase::Closed;
        }
    }

    fn queue_leave(&mut self, pending_leaves: &mut VecDeque<PendingLeave>) {
        if let Some(id) = self.player_id.take() {
            let sequence = self.sequence.saturating_add(1);
            pending_leaves.push_back(PendingLeave { id, sequence });
        }
        self.outbound_sender = None;
        self.phase = Phase::Closed;
    }

    fn ensure_outbound(&mut self) {
        if self.outbound_sender.is_none() {
            let (sender, receiver) = self.outbound.client_queue();
            self.outbound_sender = Some(sender);
            self.outbound_receiver = Some(receiver);
        }
    }

    fn should_remove(&self) -> bool {
        self.phase == Phase::Closed
    }
}

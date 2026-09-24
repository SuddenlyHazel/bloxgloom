//! Single-threaded nonblocking socket reactor.
//!
//! Every accepted socket is polled by one fixed reactor thread. Disk-backed
//! inventory loads and payload codecs are handed to fixed bounded worker
//! pools; no thread is created per client. Reads and writes have per-connection
//! budgets so one busy peer cannot monopolize a reactor pass.

use super::ContentHandshake;
mod codec;
mod connection;
mod metrics;
use crate::inventory::{Inventory, InventoryStore};
use crate::server::{INPUT_CAPACITY, MAX_CLIENTS, SimulationInput, State, run_simulation_ticks};
use codec::CodecWorkers;
use connection::Connection;
pub(in crate::server) use metrics::{TransportSnapshot, TransportStats};
use polling::{Event as PollEvent, Events, Poller};
use std::collections::VecDeque;
use std::io::{self, ErrorKind};
use std::net::{Shutdown, TcpListener};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const IO_POLL_TIMEOUT: Duration = Duration::from_millis(10);
const READ_BUDGET: usize = 64 * 1024;
const WRITE_BUDGET: usize = 64 * 1024;
const SOCKET_CHUNK: usize = 8 * 1024;
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

/// Runs the nonblocking reactor and state coordinator. Socket I/O, frame
/// boundary detection, and queue draining happen on this thread; bounded
/// workers decode and encode frame payloads.
pub(super) fn serve_listener(listener: TcpListener, state: Box<State>) -> io::Result<()> {
    serve_listener_inner(listener, state, None, Arc::new(TransportStats::default()))
}

#[cfg(test)]
pub(super) fn serve_listener_until(
    listener: TcpListener,
    state: Box<State>,
    stop: Receiver<()>,
) -> io::Result<()> {
    serve_listener_inner(
        listener,
        state,
        Some(stop),
        Arc::new(TransportStats::default()),
    )
}

pub(in crate::server) fn serve_listener_with_stats(
    listener: TcpListener,
    state: Box<State>,
    stop: Receiver<()>,
    stats: Arc<TransportStats>,
) -> io::Result<()> {
    serve_listener_inner(listener, state, Some(stop), stats)
}

fn serve_listener_inner(
    listener: TcpListener,
    state: Box<State>,
    stop: Option<Receiver<()>>,
    stats: Arc<TransportStats>,
) -> io::Result<()> {
    let admission_limit = state.admission_limit;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let inventory_store = state.inventory_store.clone();
    let outbound = Arc::clone(&state.outbound);
    let content = ContentHandshake::from_catalog(state.world.catalog_arc())?;
    let workers = InventoryWorkers::new(inventory_store)?;
    let codecs = CodecWorkers::new(
        Arc::clone(&content.catalog),
        admission_limit,
        Arc::clone(&stats),
    )?;
    let poller = Poller::new()?;
    // SAFETY: the listener remains alive until explicitly removed below.
    unsafe {
        poller.add(&listener, PollEvent::readable(LISTENER_KEY))?;
    }
    let (input_sender, input_receiver) = mpsc::sync_channel(INPUT_CAPACITY);
    let coordinator = thread::Builder::new()
        .name("server-coordinator".into())
        .spawn(move || run_simulation_ticks(*state, input_receiver))?;

    eprintln!("Bloxgloom server listening on {address}");
    let mut connections = Vec::with_capacity(admission_limit);
    let mut pending_leaves = VecDeque::with_capacity(MAX_PENDING_LEAVES);
    let mut next_connection_key = LISTENER_KEY + 1;
    let mut reactor_error = None;
    let mut events = Events::new();

    while !coordinator.is_finished() {
        if stop.as_ref().is_some_and(|receiver| {
            matches!(
                receiver.try_recv(),
                Ok(()) | Err(TryRecvError::Disconnected)
            )
        }) {
            break;
        }
        events.clear();
        if let Err(error) = poller.wait(&mut events, Some(IO_POLL_TIMEOUT)) {
            reactor_error = Some(error);
            break;
        }
        let pass_started = Instant::now();

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
                            stats.admission_rejected();
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
                        connections.push(Connection::new(
                            socket,
                            Arc::clone(&outbound),
                            key,
                            Arc::clone(&stats),
                        ));
                        stats.accepted();
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
                &codecs,
                &input_sender,
                &mut pending_leaves,
                readiness,
            );
            match result {
                Ok(()) => {}
                Err(error) => {
                    stats.disconnect(&error);
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
                stats.disconnect(&io::Error::other("poller client registration failed"));
                connections[index].disconnect(&mut pending_leaves);
            }

            if connections[index].should_remove() {
                let connection = connections.swap_remove(index);
                if let Err(error) = poller.delete(&connection.socket) {
                    eprintln!("unregister client socket: {error}");
                }
                drop(connection);
                stats.removed();
            } else {
                index += 1;
            }
        }
        stats.reactor_pass(pass_started.elapsed());
    }

    for connection in &connections {
        if let Err(error) = poller.delete(&connection.socket) {
            eprintln!("unregister client socket: {error}");
        }
        let _ = connection.socket.shutdown(Shutdown::Both);
    }
    let remaining = connections.len();
    connections.clear();
    for _ in 0..remaining {
        stats.removed();
    }
    let _ = poller.delete(&listener);
    // Best effort on listener failure; normal peer leaves are retried in FIFO
    // order until the bounded coordinator input queue accepts them.
    retry_leaves(&input_sender, &mut pending_leaves);
    drop(input_sender);
    drop(workers);
    drop(codecs);
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

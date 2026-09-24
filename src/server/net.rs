//! Listener and per-connection lifecycle for the authoritative server.
//!
//! Network threads exchange bounded messages with the simulation coordinator.
//! They never hold or inspect gameplay state.

use super::{
    INPUT_CAPACITY, JoinReply, JoinResponse, MAX_CLIENTS, OUTBOUND_CAPACITY, SimulationInput,
    State, run_simulation_ticks,
};
use crate::inventory::InventoryStore;
#[cfg(test)]
use crate::protocol::ServerMessage;
use crate::protocol::{self, ClientMessage};
use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const ACCEPT_RETRY: Duration = Duration::from_millis(2);
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const JOIN_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Owns only cloned socket handles so coordinator shutdown can wake blocked
/// readers. The lock is held only while cloning/inserting/removing handles;
/// socket I/O and thread joins happen after it is released.
#[derive(Clone, Default)]
struct SocketRegistry {
    sockets: Arc<Mutex<HashMap<u64, TcpStream>>>,
}

impl SocketRegistry {
    fn register(&self, id: u64, socket: &TcpStream) -> io::Result<()> {
        let tracked = socket.try_clone()?;
        self.sockets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, tracked);
        Ok(())
    }

    fn unregister(&self, id: u64) {
        self.sockets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id);
    }

    fn shutdown_all(&self) {
        let sockets: Vec<_> = {
            let sockets = self
                .sockets
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sockets
                .values()
                .filter_map(|socket| socket.try_clone().ok())
                .collect()
        };
        for socket in sockets {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

/// Runs a nonblocking accept loop while one dedicated thread owns all
/// simulation state. Returning from the coordinator closes every connection;
/// returning from the listener also releases the coordinator's input channel.
pub(super) fn serve_listener(listener: TcpListener, state: State) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let inventory_store = state.inventory_store.clone();
    let (input_sender, input_receiver) = mpsc::sync_channel(INPUT_CAPACITY);
    let sockets = SocketRegistry::default();
    let next_connection = AtomicU64::new(1);
    let active_connections = Arc::new(AtomicUsize::new(0));

    let coordinator_sockets = sockets.clone();
    let coordinator = thread::spawn(move || {
        let result = run_simulation_ticks(state, input_receiver);
        coordinator_sockets.shutdown_all();
        result
    });

    eprintln!("Bloxgloom server listening on {address}");
    let mut accept_error = None;
    loop {
        if coordinator.is_finished() {
            break;
        }

        match listener.accept() {
            Ok((socket, _peer)) => {
                if !reserve_connection(&active_connections) {
                    let _ = socket.shutdown(Shutdown::Both);
                    continue;
                }
                if let Err(error) = socket.set_nodelay(true) {
                    active_connections.fetch_sub(1, Ordering::AcqRel);
                    eprintln!("client socket: {error}");
                    continue;
                }

                let connection_id = next_connection.fetch_add(1, Ordering::Relaxed);
                if let Err(error) = sockets.register(connection_id, &socket) {
                    active_connections.fetch_sub(1, Ordering::AcqRel);
                    eprintln!("client socket tracking: {error}");
                    let _ = socket.shutdown(Shutdown::Both);
                    continue;
                }

                let input = input_sender.clone();
                let store = inventory_store.clone();
                let sockets = sockets.clone();
                let active_connections = Arc::clone(&active_connections);
                thread::spawn(move || {
                    if let Err(error) = serve_client(socket, store, input) {
                        eprintln!("client: {error}");
                    }
                    sockets.unregister(connection_id);
                    active_connections.fetch_sub(1, Ordering::AcqRel);
                });
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_RETRY);
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {
                thread::sleep(ACCEPT_RETRY);
            }
            Err(error) => {
                accept_error = Some(error);
                break;
            }
        }
    }

    // No lock is held while shutting down sockets or joining the coordinator.
    sockets.shutdown_all();
    drop(input_sender);
    let coordinator_result = join_coordinator(coordinator);
    match accept_error {
        Some(error) => Err(error),
        None => coordinator_result,
    }
}

fn reserve_connection(active: &AtomicUsize) -> bool {
    active
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < MAX_CLIENTS).then_some(count + 1)
        })
        .is_ok()
}

fn join_coordinator(coordinator: JoinHandle<io::Result<()>>) -> io::Result<()> {
    coordinator
        .join()
        .map_err(|_| io::Error::other("simulation coordinator panicked"))?
}

pub(super) fn serve_client(
    mut socket: TcpStream,
    inventory_store: InventoryStore,
    input: SyncSender<SimulationInput>,
) -> io::Result<()> {
    // On macOS, an accepted stream inherits the listener's nonblocking mode.
    // The connection reader uses blocking framed reads, so normalize the
    // stream before the Hello handshake or an idle read returns EAGAIN.
    socket.set_nonblocking(false)?;
    socket.set_read_timeout(Some(HELLO_TIMEOUT))?;
    let ClientMessage::Hello {
        name,
        profile,
        content_fingerprint,
    } = protocol::read_client(&mut socket)?
    else {
        return Err(io::Error::new(ErrorKind::InvalidData, "expected Hello"));
    };
    if content_fingerprint != crate::content::catalog().fingerprint() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "client content catalog does not match server",
        ));
    }
    if name.is_empty() || name.chars().any(char::is_control) || profile == 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid player name",
        ));
    }

    // Disk access stays on this connection thread. A delayed Join may outlive
    // both a newer WAL commit and its BGIN checkpoint, so the coordinator can
    // request a fresh read instead of trusting this captured snapshot.
    let join_deadline = Instant::now() + JOIN_TIMEOUT;
    let mut loaded_inventory = inventory_store.load(profile)?;
    socket.set_read_timeout(None)?;
    let (sender, receiver) = mpsc::sync_channel(OUTBOUND_CAPACITY);
    let JoinReply { id } = loop {
        let remaining = join_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(ErrorKind::TimedOut, "server join timed out"));
        }
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        input
            .try_send(SimulationInput::Join {
                profile,
                inventory: loaded_inventory,
                sender: sender.clone(),
                socket: socket.try_clone()?,
                reply: reply_sender,
            })
            .map_err(|_| io::Error::new(ErrorKind::WouldBlock, "server input queue unavailable"))?;
        match reply_receiver.recv_timeout(remaining) {
            Ok(JoinResponse::Completed(result)) => break (*result)?,
            Ok(JoinResponse::RefreshInventory) => {
                loaded_inventory = inventory_store.load(profile)?;
            }
            Err(_) => return Err(io::Error::new(ErrorKind::TimedOut, "server join timed out")),
        }
    };
    let mut join_cleanup = JoinCleanup {
        id,
        leave_sequence: 1,
        input: input.clone(),
        armed: true,
    };

    let mut write_socket = socket.try_clone()?;
    write_socket.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let writer = thread::spawn(move || {
        for mut frame in receiver {
            frame.dequeue();
            if protocol::write_server(&mut write_socket, frame.message()).is_err() {
                let _ = write_socket.shutdown(Shutdown::Both);
                break;
            }
            frame.record_sent();
        }
    });

    let mut sequence = 0u64;
    let result = loop {
        match protocol::read_client(&mut socket) {
            Ok(message) => {
                let Some(next_sequence) = sequence.checked_add(1) else {
                    break Err(io::Error::other("client command sequence exhausted"));
                };
                sequence = next_sequence;
                if input
                    .try_send(SimulationInput::Command {
                        id,
                        sequence,
                        message,
                    })
                    .is_err()
                {
                    break Err(io::Error::new(
                        ErrorKind::WouldBlock,
                        "server input queue unavailable",
                    ));
                }
                join_cleanup.leave_sequence = sequence.saturating_add(1);
            }
            Err(error) => break Err(error),
        }
    };

    let _ = socket.shutdown(Shutdown::Both);
    join_cleanup.leave_now();
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

struct JoinCleanup {
    id: u64,
    leave_sequence: u64,
    input: SyncSender<SimulationInput>,
    armed: bool,
}

impl JoinCleanup {
    fn leave_now(&mut self) {
        if !self.armed {
            return;
        }
        // A blocking send is confined to this socket thread. It preserves the
        // Leave after any queued commands when the bounded queue is briefly full.
        let _ = self.input.send(SimulationInput::Leave {
            id: self.id,
            sequence: self.leave_sequence,
        });
        self.armed = false;
    }
}

impl Drop for JoinCleanup {
    fn drop(&mut self) {
        self.leave_now();
    }
}

#[cfg(test)]
#[path = "net/tests.rs"]
mod tests;

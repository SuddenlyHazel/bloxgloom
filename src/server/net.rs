//! Listener and per-connection lifecycle for the authoritative server.
//!
//! The socket reactor exchanges bounded messages with the simulation
//! coordinator and never holds or inspects gameplay state.

use super::State;
use crate::content::{Catalog, ContentManifest};
#[cfg(test)]
use crate::inventory::InventoryStore;
#[cfg(test)]
use crate::protocol::ClientMessage;
use crate::protocol::{self, MAX_MANIFEST_PART, ServerMessage};
use std::io;
#[cfg(test)]
use std::io::{ErrorKind, Write};
use std::net::TcpListener;
#[cfg(test)]
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
#[cfg(test)]
use std::sync::mpsc::{self, SyncSender};
#[cfg(test)]
use std::thread;
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

mod bundle;
mod reactor;
pub(in crate::server) use reactor::{TransportSnapshot, TransportStats};

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const JOIN_TIMEOUT: Duration = Duration::from_secs(5);
const BUNDLE_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(test)]
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Shared encoded manifest sent to every connection before it can join. The
/// catalog is frozen at startup, so encoding once keeps the handshake bounded
/// without doing manifest work on each socket thread.
#[derive(Clone)]
pub(super) struct ContentHandshake {
    fingerprint: u64,
    manifest_frames: Arc<[Arc<[u8]>]>,
    catalog: Arc<Catalog>,
    bundle: Option<Arc<bundle::BundleHandshake>>,
}

impl ContentHandshake {
    #[cfg(test)]
    pub(super) fn from_catalog(catalog: Arc<Catalog>) -> io::Result<Arc<Self>> {
        Self::with_bundle(catalog, None)
    }

    fn with_bundle(
        catalog: Arc<Catalog>,
        bundle: Option<&super::script::package::client::ClientBundle>,
    ) -> io::Result<Arc<Self>> {
        let bundle = bundle
            .map(|bundle| bundle::BundleHandshake::new(bundle, &catalog))
            .transpose()?
            .map(Arc::new);
        let manifest = ContentManifest::from_catalog(&catalog).encode()?;
        let fingerprint = catalog.fingerprint();
        let total_len = u32::try_from(manifest.len())
            .map_err(|_| io::Error::other("content manifest length exceeds wire limit"))?;
        let mut manifest_frames = Vec::new();
        for offset in (0..manifest.len()).step_by(MAX_MANIFEST_PART) {
            let end = (offset + MAX_MANIFEST_PART).min(manifest.len());
            let offset_wire = u32::try_from(offset)
                .map_err(|_| io::Error::other("content manifest offset exceeds wire limit"))?;
            let part = ServerMessage::ContentManifestPart {
                fingerprint,
                total_len,
                offset: offset_wire,
                bytes: manifest[offset..end].to_vec(),
            };
            let mut frame = Vec::new();
            protocol::write_server_with_catalog(&mut frame, &part, &catalog)?;
            manifest_frames.push(Arc::from(frame));
        }
        Ok(Arc::new(Self {
            bundle,
            fingerprint,
            manifest_frames: Arc::from(manifest_frames),
            catalog,
        }))
    }

    #[cfg(test)]
    pub(super) fn from_local_catalog() -> io::Result<Arc<Self>> {
        Self::from_catalog(Arc::new(crate::content::catalog().clone()))
    }

    #[cfg(test)]
    fn send_parts(&self, socket: &mut TcpStream) -> io::Result<()> {
        for frame in self.manifest_frames.iter() {
            socket.write_all(frame)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn validate_ready(&self, message: ClientMessage) -> io::Result<()> {
        match message {
            ClientMessage::ContentReady { fingerprint } if fingerprint == self.fingerprint => {
                Ok(())
            }
            _ => Err(io::Error::new(
                ErrorKind::InvalidData,
                "expected matching ContentReady before joining",
            )),
        }
    }
}

/// Runs the fixed-thread nonblocking socket reactor alongside the coordinator.
pub(super) fn serve_listener(listener: TcpListener, state: Box<State>) -> io::Result<()> {
    reactor::serve_listener(listener, state)
}

pub(in crate::server) fn serve_listener_with_stats(
    listener: TcpListener,
    state: Box<State>,
    stop: std::sync::mpsc::Receiver<()>,
    stats: Arc<TransportStats>,
) -> io::Result<()> {
    reactor::serve_listener_with_stats(listener, state, stop, stats)
}

#[cfg(test)]
use super::{JoinReply, JoinResponse, SimulationInput};
#[cfg(test)]
pub(super) fn serve_client(
    mut socket: TcpStream,
    inventory_store: InventoryStore,
    input: SyncSender<SimulationInput>,
    outbound: Arc<super::outbound::OutboundTelemetry>,
    content: Arc<ContentHandshake>,
) -> io::Result<()> {
    // On macOS, an accepted stream inherits the listener's nonblocking mode.
    // The connection reader uses blocking framed reads, so normalize the
    // stream before the Hello handshake or an idle read returns EAGAIN.
    socket.set_nonblocking(false)?;
    socket.set_read_timeout(Some(HELLO_TIMEOUT))?;
    let ClientMessage::Hello {
        name,
        profile,
        content_fingerprint: _,
    } = protocol::read_client_with_catalog(&mut socket, &content.catalog)?
    else {
        return Err(io::Error::new(ErrorKind::InvalidData, "expected Hello"));
    };
    if name.is_empty() || name.chars().any(char::is_control) || profile == 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid player name",
        ));
    }

    socket.set_write_timeout(Some(HELLO_TIMEOUT))?;
    content.send_parts(&mut socket)?;
    content.validate_ready(protocol::read_client_with_catalog(
        &mut socket,
        &content.catalog,
    )?)?;
    socket.set_read_timeout(None)?;
    socket.set_write_timeout(None)?;

    // Disk access stays on this connection thread. A delayed Join may outlive
    // both a newer WAL commit and its BGIN checkpoint, so the coordinator can
    // request a fresh read instead of trusting this captured snapshot.
    let join_deadline = Instant::now() + JOIN_TIMEOUT;
    let mut loaded_inventory = inventory_store.load(profile)?;
    let (sender, receiver) = outbound.client_queue();
    let JoinReply { id } = loop {
        let remaining = join_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(ErrorKind::TimedOut, "server join timed out"));
        }
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        input
            .try_send(SimulationInput::Join {
                name: name.clone(),
                profile,
                inventory: Box::new(loaded_inventory),
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
    let writer_catalog = Arc::clone(&content.catalog);
    let writer = thread::spawn(move || {
        for frame in receiver {
            if protocol::write_server_with_catalog(
                &mut write_socket,
                frame.message(),
                &writer_catalog,
            )
            .is_err()
            {
                let _ = write_socket.shutdown(Shutdown::Both);
                break;
            }
            frame.record_sent();
        }
    });

    let mut sequence = 0u64;
    let result = loop {
        match protocol::read_client_with_catalog(&mut socket, &content.catalog) {
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

#[cfg(test)]
struct JoinCleanup {
    id: u64,
    leave_sequence: u64,
    input: SyncSender<SimulationInput>,
    armed: bool,
}

#[cfg(test)]
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

#[cfg(test)]
impl Drop for JoinCleanup {
    fn drop(&mut self) {
        self.leave_now();
    }
}

#[cfg(test)]
#[path = "net/tests.rs"]
mod tests;

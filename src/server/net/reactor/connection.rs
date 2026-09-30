//! Per-socket framing, handshake, and ordered command lifecycle.

use super::codec::{CodecWorkers, EncodedFrame, SubmitError};
use super::metrics::TransportStats;
use super::{InventoryWorkers, PendingLeave, READ_BUDGET, Readiness, SOCKET_CHUNK, WRITE_BUDGET};
use crate::inventory::Inventory;
use crate::protocol::{ClientMessage, MAX_FRAME};
use crate::server::net::{ContentHandshake, HELLO_TIMEOUT, JOIN_TIMEOUT};
use crate::server::outbound::{OutboundFrame, OutboundQueue, OutboundTelemetry};
use crate::server::{JoinReply, JoinResponse, SimulationInput};
use polling::{Event as PollEvent, Poller};
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::Instant;

#[cfg(test)]
#[path = "connection/tests.rs"]
mod tests;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    AwaitHello,
    SendingManifest,
    AwaitContentReady,
    SendingBundleOffer,
    AwaitBundleRequest,
    SendingBundle,
    AwaitBundleReady,
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
    BundleOffer,
    BundlePart,
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

    fn outbound(bytes: Arc<[u8]>, reservation: OutboundFrame) -> Self {
        Self {
            bytes,
            offset: 0,
            kind: PendingWriteKind::Outbound,
            reservation: Some(reservation),
        }
    }
}

pub(super) struct Connection {
    pub(super) socket: TcpStream,
    pub(super) poll_key: usize,
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
    bundle_index: usize,
    pending_write: Option<PendingWrite>,
    pending_decode_frame: Option<Vec<u8>>,
    decode_receiver: Option<Receiver<io::Result<ClientMessage>>>,
    pending_command: Option<ClientMessage>,
    pending_encode_frame: Option<OutboundFrame>,
    encode_receiver: Option<Receiver<io::Result<EncodedFrame>>>,
    inventory: Option<Inventory>,
    inventory_receiver: Option<Receiver<io::Result<Inventory>>>,
    join_receiver: Option<Receiver<JoinResponse>>,
    outbound: Arc<OutboundTelemetry>,
    outbound_sender: Option<OutboundQueue>,
    outbound_receiver: Option<Receiver<OutboundFrame>>,
    stats: Arc<TransportStats>,
}

impl Connection {
    pub(super) fn new(
        socket: TcpStream,
        outbound: Arc<OutboundTelemetry>,
        poll_key: usize,
        stats: Arc<TransportStats>,
    ) -> Self {
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
            bundle_index: 0,
            pending_write: None,
            pending_decode_frame: None,
            decode_receiver: None,
            pending_command: None,
            pending_encode_frame: None,
            encode_receiver: None,
            inventory: None,
            inventory_receiver: None,
            join_receiver: None,
            outbound,
            outbound_sender: None,
            outbound_receiver: None,
            stats,
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Explicit borrowed reactor services keep connection polling independent of reactor ownership."
    )]
    pub(super) fn poll(
        &mut self,
        now: Instant,
        content: &ContentHandshake,
        workers: &InventoryWorkers,
        codecs: &CodecWorkers,
        input: &SyncSender<SimulationInput>,
        pending_leaves: &mut VecDeque<PendingLeave>,
        readiness: Readiness,
    ) -> io::Result<()> {
        if self.phase == Phase::Closed {
            return Ok(());
        }
        if now >= self.deadline && self.phase != Phase::Active {
            if self.phase == Phase::AwaitJoin {
                tracing::debug!(player_id = ?self.player_id, connection_key = self.poll_key, "connection closing");
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
        self.flush_pending_command(input)?;
        if self.pending_command.is_none() {
            self.poll_decode(now, content, input)?;
        }
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
        if readiness.readable
            || (!self.input_buffer.is_empty() && self.decode_receiver.is_none())
            || self.pending_decode_frame.is_some()
        {
            self.poll_read(codecs)?;
        }
        self.poll_async(now, workers, input, pending_leaves)?;
        self.prepare_write(content, codecs)?;
        // A newly encoded frame should not wait for another socket edge or
        // the reactor timeout. One frame is written per pass; the next encode
        // is submitted immediately and its worker wakes the poller.
        if self.pending_write.is_some() && (readiness.writable || !self.write_interest) {
            self.poll_write()?;
            if self.pending_write.is_none() && self.encode_receiver.is_none() {
                self.prepare_write(content, codecs)?;
            }
        }
        Ok(())
    }

    pub(super) fn update_interest(&mut self, poller: &Poller, force_rearm: bool) -> io::Result<()> {
        let waiting_for_join = matches!(
            self.phase,
            Phase::ReadyToLoadInventory
                | Phase::LoadingInventory
                | Phase::ReadyToJoin
                | Phase::AwaitJoin
        );
        let read_interest = !self.peer_closed
            && self.phase != Phase::Closed
            && !waiting_for_join
            && self.decode_receiver.is_none()
            && self.pending_decode_frame.is_none()
            && self.pending_command.is_none();
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
                if let Ok(receiver) = workers.request(profile) {
                    self.inventory_receiver = Some(receiver);
                    self.phase = Phase::LoadingInventory;
                    progress = true;
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
                    inventory: Box::new(inventory),
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
                        self.inventory = Some(*inventory);
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
                            tracing::info!(
                                player_id = id,
                                connection_key = self.poll_key,
                                "player joined"
                            );
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

    fn prepare_write(
        &mut self,
        content: &ContentHandshake,
        codecs: &CodecWorkers,
    ) -> io::Result<()> {
        if self.pending_write.is_none() {
            match self.phase {
                Phase::SendingBundleOffer => {
                    let bundle = content.bundle.as_ref().expect("bundle required");
                    self.pending_write = Some(PendingWrite {
                        bytes: Arc::clone(&bundle.offer),
                        offset: 0,
                        kind: PendingWriteKind::BundleOffer,
                        reservation: None,
                    });
                }
                Phase::SendingBundle => {
                    let bundle = content.bundle.as_ref().expect("bundle required");
                    if let Some(frame) = bundle.parts.get(self.bundle_index) {
                        self.pending_write = Some(PendingWrite {
                            bytes: Arc::clone(frame),
                            offset: 0,
                            kind: PendingWriteKind::BundlePart,
                            reservation: None,
                        });
                    } else {
                        self.phase = Phase::AwaitBundleReady;
                    }
                }
                Phase::SendingManifest => {
                    if let Some(frame) = content.manifest_frames.get(self.manifest_index) {
                        self.pending_write = Some(PendingWrite::manifest(Arc::clone(frame)));
                    } else {
                        self.phase = Phase::AwaitContentReady;
                        self.deadline = Instant::now() + HELLO_TIMEOUT;
                    }
                }
                Phase::Active => {
                    if let Some(receiver) = self.encode_receiver.as_ref() {
                        match receiver.try_recv() {
                            Ok(Ok(EncodedFrame { bytes, reservation })) => {
                                self.encode_receiver = None;
                                self.pending_write =
                                    Some(PendingWrite::outbound(bytes, reservation));
                                return Ok(());
                            }
                            Ok(Err(error)) => return Err(error),
                            Err(TryRecvError::Empty) => return Ok(()),
                            Err(TryRecvError::Disconnected) => {
                                return Err(io::Error::other("server encoder worker stopped"));
                            }
                        }
                    }
                    let frame = match self.pending_encode_frame.take() {
                        Some(frame) => frame,
                        None => {
                            let receiver = self
                                .outbound_receiver
                                .as_ref()
                                .expect("active connection has an outbound receiver");
                            match receiver.try_recv() {
                                Ok(frame) => frame,
                                Err(TryRecvError::Empty) => return Ok(()),
                                Err(TryRecvError::Disconnected) => {
                                    return Err(io::Error::new(
                                        ErrorKind::BrokenPipe,
                                        "client outbound queue closed",
                                    ));
                                }
                            }
                        }
                    };
                    match codecs.encode(frame) {
                        Ok(receiver) => self.encode_receiver = Some(receiver),
                        Err(SubmitError::Full(frame)) => self.pending_encode_frame = Some(frame),
                        Err(SubmitError::Closed(_)) => {
                            return Err(io::Error::other("server encoder pool closed"));
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
                    self.stats.output(count);
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
            } else if completed.kind == PendingWriteKind::BundleOffer {
                self.phase = Phase::AwaitBundleRequest;
            } else if completed.kind == PendingWriteKind::BundlePart {
                self.bundle_index += 1;
            } else if let Some(frame) = completed.reservation.take() {
                self.stats.send_age(frame.age());
                frame.record_sent();
                drop(frame);
            }
        }
        Ok(())
    }

    fn poll_decode(
        &mut self,
        now: Instant,
        content: &ContentHandshake,
        input: &SyncSender<SimulationInput>,
    ) -> io::Result<bool> {
        let Some(receiver) = self.decode_receiver.as_ref() else {
            return Ok(false);
        };
        match receiver.try_recv() {
            Ok(Ok(message)) => {
                self.decode_receiver = None;
                self.handle_message(now, message, content, input)?;
                Ok(true)
            }
            Ok(Err(error)) => Err(error),
            Err(TryRecvError::Empty) => Ok(false),
            Err(TryRecvError::Disconnected) => {
                Err(io::Error::other("server decoder worker stopped"))
            }
        }
    }

    fn flush_pending_command(&mut self, input: &SyncSender<SimulationInput>) -> io::Result<()> {
        let Some(message) = self.pending_command.take() else {
            return Ok(());
        };
        self.enqueue_command(input, message)
    }

    fn enqueue_command(
        &mut self,
        input: &SyncSender<SimulationInput>,
        message: ClientMessage,
    ) -> io::Result<()> {
        let id = self.player_id.expect("active connection has a player ID");
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("client command sequence exhausted"))?;
        match input.try_send(SimulationInput::Command {
            id,
            sequence,
            message,
        }) {
            Ok(()) => {
                self.sequence = sequence;
                Ok(())
            }
            Err(TrySendError::Full(SimulationInput::Command { message, .. })) => {
                self.pending_command = Some(message);
                Ok(())
            }
            Err(TrySendError::Disconnected(_)) => Err(io::Error::new(
                ErrorKind::BrokenPipe,
                "simulation coordinator stopped",
            )),
            Err(TrySendError::Full(_)) => unreachable!("command enqueue only sends Command"),
        }
    }

    fn poll_read(&mut self, codecs: &CodecWorkers) -> io::Result<bool> {
        if matches!(
            self.phase,
            Phase::ReadyToLoadInventory
                | Phase::LoadingInventory
                | Phase::ReadyToJoin
                | Phase::AwaitJoin
        ) {
            // ContentReady (and, when required, BundleReady) is the gate.
            // Later bytes remain in the bounded parser buffer until Join
            // completes; commands are never submitted against a player ID that
            // has not been created.
            return Ok(false);
        }
        if self.decode_receiver.is_some() || self.pending_command.is_some() {
            return Ok(false);
        }
        let mut progress = false;
        let mut bytes_read = 0;
        loop {
            let frame = match self.pending_decode_frame.take() {
                Some(frame) => Some(frame),
                None => self.take_complete_frame()?,
            };
            if let Some(frame) = frame {
                match codecs.decode(frame) {
                    Ok(receiver) => self.decode_receiver = Some(receiver),
                    Err(SubmitError::Full(frame)) => self.pending_decode_frame = Some(frame),
                    Err(SubmitError::Closed(_)) => {
                        return Err(io::Error::other("server decoder pool closed"));
                    }
                }
                return Ok(true);
            }
            if bytes_read >= READ_BUDGET {
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
                    self.stats.input(count);
                    if self.input_buffer.len() > MAX_FRAME + 4 + SOCKET_CHUNK {
                        return Err(io::Error::new(
                            ErrorKind::InvalidData,
                            "client parser buffer exceeded bound",
                        ));
                    }
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
                self.phase = if content.bundle.is_some() {
                    Phase::SendingBundleOffer
                } else {
                    Phase::SendingManifest
                };
                // Transfer precedes catalog validation so clients can obtain
                // artifacts without already having the package definitions.
                self.deadline = now
                    + if content.bundle.is_some() {
                        super::super::BUNDLE_TIMEOUT
                    } else {
                        HELLO_TIMEOUT
                    };
                Ok(())
            }
            (Phase::AwaitContentReady, ClientMessage::ContentReady { fingerprint })
                if fingerprint == content.fingerprint =>
            {
                self.phase = Phase::ReadyToLoadInventory;
                self.deadline = now + JOIN_TIMEOUT;
                Ok(())
            }
            (Phase::AwaitBundleRequest, ClientMessage::BundleRequest { identity })
                if content
                    .bundle
                    .as_ref()
                    .is_some_and(|bundle| bundle.identity == identity) =>
            {
                // The absolute deadline established by Hello is not extended
                // by requests or part progress.
                self.phase = Phase::SendingBundle;
                Ok(())
            }
            (
                Phase::AwaitBundleRequest | Phase::AwaitBundleReady,
                ClientMessage::BundleReady { identity },
            ) if content
                .bundle
                .as_ref()
                .is_some_and(|bundle| bundle.identity == identity) =>
            {
                self.phase = Phase::SendingManifest;
                self.deadline = now + HELLO_TIMEOUT;
                Ok(())
            }
            (
                Phase::Active,
                ClientMessage::Hello { .. }
                | ClientMessage::ContentReady { .. }
                | ClientMessage::BundleRequest { .. }
                | ClientMessage::BundleReady { .. },
            ) => Err(io::Error::new(
                ErrorKind::InvalidData,
                "duplicate content handshake message",
            )),
            (Phase::Active, message) => self.enqueue_command(input, message),
            _ => Err(io::Error::new(
                ErrorKind::InvalidData,
                "unexpected message for connection state",
            )),
        }
    }

    pub(super) fn disconnect(&mut self, pending_leaves: &mut VecDeque<PendingLeave>) {
        if self.peer_closed || self.phase == Phase::Closed {
            return;
        }
        tracing::debug!(player_id = ?self.player_id, connection_key = self.poll_key, "connection closing");
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

    pub(super) fn should_remove(&self) -> bool {
        self.phase == Phase::Closed
    }
}

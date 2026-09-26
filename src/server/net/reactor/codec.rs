//! Bounded frame codecs. Sockets, ordering, and admissions remain reactor-owned.

use super::metrics::TransportStats;
use crate::content::Catalog;
use crate::protocol::{self, ClientMessage};
use crate::server::outbound::OutboundFrame;
use polling::Poller;
use std::io;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

const DECODE_WORKERS: usize = 4;
const ENCODE_WORKERS: usize = 4;

struct DecodeRequest {
    frame: Vec<u8>,
    reply: SyncSender<io::Result<ClientMessage>>,
}

struct EncodeRequest {
    frame: OutboundFrame,
    reply: SyncSender<io::Result<EncodedFrame>>,
}

pub(super) struct EncodedFrame {
    pub(super) bytes: Arc<[u8]>,
    pub(super) reservation: OutboundFrame,
}

#[derive(Debug)]
pub(super) enum SubmitError<T> {
    Full(T),
    Closed(T),
}

pub(super) struct CodecWorkers {
    decode_sender: Option<SyncSender<DecodeRequest>>,
    decode_workers: Vec<JoinHandle<()>>,
    encode_sender: Option<SyncSender<EncodeRequest>>,
    encode_workers: Vec<JoinHandle<()>>,
    stats: Arc<TransportStats>,
}

impl CodecWorkers {
    pub(super) fn new(
        catalog: Arc<Catalog>,
        capacity: usize,
        stats: Arc<TransportStats>,
        poller: Arc<Poller>,
    ) -> io::Result<Self> {
        if capacity == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "zero codec queue capacity",
            ));
        }
        let (decode_sender, decode_receiver) = mpsc::sync_channel(capacity);
        let (encode_sender, encode_receiver) = mpsc::sync_channel(capacity);
        let decode_receiver = Arc::new(Mutex::new(decode_receiver));
        let encode_receiver = Arc::new(Mutex::new(encode_receiver));
        let mut pool = Self {
            decode_sender: Some(decode_sender),
            decode_workers: Vec::with_capacity(DECODE_WORKERS),
            encode_sender: Some(encode_sender),
            encode_workers: Vec::with_capacity(ENCODE_WORKERS),
            stats: Arc::clone(&stats),
        };
        for index in 0..DECODE_WORKERS {
            let receiver = Arc::clone(&decode_receiver);
            let catalog = Arc::clone(&catalog);
            let stats = Arc::clone(&stats);
            let poller = Arc::clone(&poller);
            pool.decode_workers.push(
                thread::Builder::new()
                    .name(format!("server-decode-{index}"))
                    .spawn(move || decode_worker(receiver, catalog, stats, poller))?,
            );
        }
        for index in 0..ENCODE_WORKERS {
            let receiver = Arc::clone(&encode_receiver);
            let catalog = Arc::clone(&catalog);
            let stats = Arc::clone(&stats);
            let poller = Arc::clone(&poller);
            pool.encode_workers.push(
                thread::Builder::new()
                    .name(format!("server-encode-{index}"))
                    .spawn(move || encode_worker(receiver, catalog, stats, poller))?,
            );
        }
        Ok(pool)
    }

    pub(super) fn decode(
        &self,
        frame: Vec<u8>,
    ) -> Result<Receiver<io::Result<ClientMessage>>, SubmitError<Vec<u8>>> {
        let (reply, receiver) = mpsc::sync_channel(1);
        let request = DecodeRequest { frame, reply };
        self.stats.decode_submitted();
        match self
            .decode_sender
            .as_ref()
            .expect("codec pool active")
            .try_send(request)
        {
            Ok(()) => Ok(receiver),
            Err(TrySendError::Full(request)) => {
                self.stats.decode_rejected();
                Err(SubmitError::Full(request.frame))
            }
            Err(TrySendError::Disconnected(request)) => {
                self.stats.decode_rejected();
                Err(SubmitError::Closed(request.frame))
            }
        }
    }

    pub(super) fn encode(
        &self,
        frame: OutboundFrame,
    ) -> Result<Receiver<io::Result<EncodedFrame>>, SubmitError<OutboundFrame>> {
        let (reply, receiver) = mpsc::sync_channel(1);
        let request = EncodeRequest { frame, reply };
        self.stats.encode_submitted();
        match self
            .encode_sender
            .as_ref()
            .expect("codec pool active")
            .try_send(request)
        {
            Ok(()) => Ok(receiver),
            Err(TrySendError::Full(request)) => {
                self.stats.encode_rejected();
                Err(SubmitError::Full(request.frame))
            }
            Err(TrySendError::Disconnected(request)) => {
                self.stats.encode_rejected();
                Err(SubmitError::Closed(request.frame))
            }
        }
    }
}

impl Drop for CodecWorkers {
    fn drop(&mut self) {
        drop(self.decode_sender.take());
        drop(self.encode_sender.take());
        for worker in self
            .decode_workers
            .drain(..)
            .chain(self.encode_workers.drain(..))
        {
            let _ = worker.join();
        }
    }
}

fn decode_worker(
    receiver: Arc<Mutex<Receiver<DecodeRequest>>>,
    catalog: Arc<Catalog>,
    stats: Arc<TransportStats>,
    poller: Arc<Poller>,
) {
    loop {
        let request = receiver
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv();
        let Ok(request) = request else {
            return;
        };
        let started = Instant::now();
        let result = protocol::read_client_with_catalog(&request.frame[..], &catalog);
        stats.decode_busy(started.elapsed());
        let _ = request.reply.try_send(result);
        stats.decode_finished();
        let _ = poller.notify();
    }
}

fn encode_worker(
    receiver: Arc<Mutex<Receiver<EncodeRequest>>>,
    catalog: Arc<Catalog>,
    stats: Arc<TransportStats>,
    poller: Arc<Poller>,
) {
    loop {
        let request = receiver
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv();
        let Ok(request) = request else {
            return;
        };
        let started = Instant::now();
        let result = request.frame.encode(&catalog).map(|bytes| EncodedFrame {
            bytes,
            reservation: request.frame,
        });
        stats.encode_busy(started.elapsed());
        let _ = request.reply.try_send(result);
        stats.encode_finished();
        let _ = poller.notify();
    }
}

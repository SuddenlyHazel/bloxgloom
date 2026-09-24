use crate::config::Config;
use crate::lighting::LightField;
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::render::{self, ChunkMesh};
use crate::world::{Chunk, ChunkKey};
use std::collections::HashMap;
use std::io;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;

pub(super) enum Incoming {
    Message(ServerMessage),
    Closed(String),
}

pub(super) struct Network {
    pub(super) incoming: Receiver<Incoming>,
    outgoing: SyncSender<ClientMessage>,
}

impl Network {
    pub(super) fn connect(addr: &str, view_distance: u8, profile: u128) -> io::Result<Self> {
        let socket = TcpStream::connect(addr)?;
        socket.set_nodelay(true)?;
        let mut reader = socket.try_clone()?;
        let mut writer = socket;
        let (incoming_tx, incoming) = mpsc::sync_channel(256);
        let (outgoing, outgoing_rx) = mpsc::sync_channel(256);
        thread::spawn(move || {
            loop {
                match protocol::read_server(&mut reader) {
                    Ok(message) => {
                        if incoming_tx.send(Incoming::Message(message)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = incoming_tx.try_send(Incoming::Closed(error.to_string()));
                        break;
                    }
                }
            }
        });
        thread::spawn(move || {
            for message in outgoing_rx {
                if protocol::write_client(&mut writer, &message).is_err() {
                    break;
                }
            }
        });
        outgoing
            .send(ClientMessage::Hello {
                name: "Player".into(),
                profile,
            })
            .map_err(|_| io::Error::other("network writer stopped"))?;
        outgoing
            .send(ClientMessage::SetView {
                radius: view_distance,
            })
            .map_err(|_| io::Error::other("network writer stopped"))?;
        Ok(Self { incoming, outgoing })
    }

    pub(super) fn send(&self, message: ClientMessage) -> bool {
        match self.outgoing.try_send(message) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                eprintln!("client command queue full");
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

pub(super) struct ConfigWriter {
    current: Arc<Mutex<Config>>,
    wake: Option<SyncSender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ConfigWriter {
    pub(super) fn new(config: &Config, path: PathBuf) -> Self {
        let current = Arc::new(Mutex::new(config.clone()));
        let snapshot = Arc::clone(&current);
        let (wake, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            while receiver.recv().is_ok() {
                let config = snapshot.lock().unwrap().clone();
                if let Err(error) = config.save(&path) {
                    eprintln!("settings save: {error}");
                }
            }
        });
        Self {
            current,
            wake: Some(wake),
            worker: Some(worker),
        }
    }

    pub(super) fn request_save(&self, config: &Config) {
        *self.current.lock().unwrap() = config.clone();
        if let Some(wake) = &self.wake {
            let _ = wake.try_send(());
        }
    }

    pub(super) fn finish(&mut self) {
        self.wake.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(super) struct Mesher {
    pub(super) jobs: SyncSender<MesherJob>,
    pub(super) results: Receiver<ChunkMesh>,
}

pub(super) struct MesherJob {
    pub(super) chunk: Arc<Chunk>,
    pub(super) known: HashMap<ChunkKey, Arc<Chunk>>,
    pub(super) seed: u64,
    pub(super) revision: u64,
    pub(super) bounced_gi: bool,
}

impl Mesher {
    pub(super) fn new() -> Self {
        let (jobs, jobs_rx) = mpsc::sync_channel::<MesherJob>(64);
        let (results_tx, results) = mpsc::sync_channel(64);
        let shared = Arc::new(Mutex::new(jobs_rx));
        for _ in 0..2 {
            let jobs_rx = Arc::clone(&shared);
            let results_tx = results_tx.clone();
            thread::spawn(move || {
                loop {
                    let job = match jobs_rx.lock().unwrap().recv() {
                        Ok(job) => job,
                        Err(_) => break,
                    };
                    let light = if job.bounced_gi {
                        LightField::build_with_bounce(job.chunk.key, &job.known, job.seed, true)
                    } else {
                        LightField::build(job.chunk.key, &job.known, job.seed)
                    };
                    if results_tx
                        .send(render::mesh_chunk_lit(&job.chunk, &light, job.revision))
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        Self { jobs, results }
    }
}

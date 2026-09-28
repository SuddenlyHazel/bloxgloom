use crate::config::Config;
use crate::content::{Catalog, ContentManifest, MAX_MANIFEST_BYTES};
use crate::lighting::{LightField, LightSample};
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
use std::time::Duration;

pub(super) enum Incoming {
    Message(Box<ServerMessage>),
    Closed(String),
}

pub(super) struct Network {
    // Session-owned immutable artifact, installed before any snapshot is read.
    _bundle: Option<Arc<crate::server::client_bundle::ClientBundle>>,
    material: Option<crate::render::custom::Prepared>,
    pub(super) incoming: Receiver<Incoming>,
    pub(super) catalog: Arc<Catalog>,
    outgoing: SyncSender<ClientMessage>,
}

impl Network {
    pub(super) fn package_material(&self) -> Option<&crate::render::custom::Prepared> {
        self.material.as_ref()
    }
    pub(super) fn package_effect(&self) -> Option<&crate::render::effects::Prepared> {
        self._bundle.as_ref()?.effect().map(AsRef::as_ref)
    }
    pub(super) fn package_ui(&self) -> Option<crate::ui::authored::Session> {
        self._bundle
            .as_ref()?
            .ui()
            .map(|resources| crate::ui::authored::Session::new(Arc::clone(resources)))
    }
    #[cfg(test)]
    pub(super) fn disconnected_for_test() -> Self {
        let (_, incoming) = mpsc::sync_channel(1);
        let (outgoing, _) = mpsc::sync_channel(1);
        Self {
            _bundle: None,
            material: None,
            incoming,
            outgoing,
            catalog: Arc::new(crate::content::catalog().clone()),
        }
    }

    #[cfg(test)]
    pub(super) fn idle_for_test() -> (Self, SyncSender<Incoming>) {
        let (sender, incoming) = mpsc::sync_channel(1);
        let (outgoing, _) = mpsc::sync_channel(1);
        (
            Self {
                _bundle: None,
                material: None,
                incoming,
                outgoing,
                catalog: Arc::new(crate::content::catalog().clone()),
            },
            sender,
        )
    }

    pub(super) fn connect(addr: &str, view_distance: u8, profile: u128) -> io::Result<Self> {
        let mut socket = TcpStream::connect(addr)?;
        socket.set_nodelay(true)?;
        socket.set_read_timeout(Some(Duration::from_secs(10)))?;
        socket.set_write_timeout(Some(Duration::from_secs(10)))?;
        protocol::write_client(
            &mut socket,
            &ClientMessage::Hello {
                name: "Player".into(),
                profile,
                content_fingerprint: Catalog::builtins().fingerprint(),
            },
        )?;
        let mut first = protocol::read_server(&mut socket)?;
        let bundle = if let ServerMessage::BundleOffer { identity } = first {
            let bundle = super::bundle::install(&mut socket, identity)?;
            // Transfer time does not consume the separate content/Join
            // budget (the receiver narrows OS timeouts to its remaining time).
            socket.set_read_timeout(Some(Duration::from_secs(10)))?;
            socket.set_write_timeout(Some(Duration::from_secs(10)))?;
            first = protocol::read_server(&mut socket)?;
            Some(bundle)
        } else {
            None
        };
        let local = match &bundle {
            Some(bundle) => bundle.session_catalog()?,
            None => Catalog::builtins(),
        };
        let (content_fingerprint, catalog) = receive_content_manifest(&mut socket, first, &local)?;
        let material = bundle
            .as_ref()
            .and_then(|bundle| bundle.material())
            .map(|material| {
                material
                    .resolve(&catalog)
                    .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))
            })
            .transpose()?;
        protocol::write_client(
            &mut socket,
            &ClientMessage::ContentReady {
                fingerprint: content_fingerprint,
            },
        )?;
        let first = protocol::read_server_with_catalog(&mut socket, &catalog)?;
        if !matches!(first, ServerMessage::Welcome { .. }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected Welcome after preparation",
            ));
        }
        socket.set_read_timeout(None)?;
        socket.set_write_timeout(None)?;
        let mut reader = socket.try_clone()?;
        let mut writer = socket;
        let reader_catalog = Arc::clone(&catalog);
        let writer_catalog = Arc::clone(&catalog);
        let (incoming_tx, incoming) = mpsc::sync_channel(256);
        incoming_tx
            .send(Incoming::Message(Box::new(first)))
            .map_err(|_| io::Error::other("network incoming queue closed"))?;
        let (outgoing, outgoing_rx) = mpsc::sync_channel(256);
        thread::spawn(move || {
            loop {
                match protocol::read_server_with_catalog(&mut reader, &reader_catalog) {
                    Ok(message) => {
                        if incoming_tx
                            .send(Incoming::Message(Box::new(message)))
                            .is_err()
                        {
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
                if protocol::write_client_with_catalog(&mut writer, &message, &writer_catalog)
                    .is_err()
                {
                    break;
                }
            }
        });
        outgoing
            .send(ClientMessage::SetView {
                radius: view_distance,
            })
            .map_err(|_| io::Error::other("network writer stopped"))?;
        Ok(Self {
            _bundle: bundle,
            material,
            incoming,
            catalog,
            outgoing,
        })
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

fn receive_content_manifest(
    socket: &mut TcpStream,
    mut message: ServerMessage,
    local: &Catalog,
) -> io::Result<(u64, Arc<Catalog>)> {
    let mut bytes = Vec::new();
    let mut expected: Option<(usize, u64)> = None;
    loop {
        let ServerMessage::ContentManifestPart {
            fingerprint,
            total_len,
            offset,
            bytes: part,
        } = message
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected content manifest before Welcome",
            ));
        };
        let total = usize::try_from(total_len)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "content length overflow"))?;
        if total == 0 || total > MAX_MANIFEST_BYTES || part.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid content manifest part size",
            ));
        }
        if let Some((prior_total, prior_fingerprint)) = expected {
            if total != prior_total || fingerprint != prior_fingerprint {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "mixed content manifest parts",
                ));
            }
        } else {
            expected = Some((total, fingerprint));
            bytes.reserve(total);
        }
        if usize::try_from(offset).ok() != Some(bytes.len())
            || part.len() > total.saturating_sub(bytes.len())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "out-of-order content manifest part",
            ));
        }
        bytes.extend_from_slice(&part);
        if bytes.len() == total {
            let manifest = ContentManifest::decode(&bytes)?;
            let catalog = manifest.resolve_catalog(local)?;
            if catalog.fingerprint() != fingerprint {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "content manifest fingerprint mismatch",
                ));
            }
            return Ok((fingerprint, Arc::new(catalog)));
        }
        message = protocol::read_server(&mut *socket)?;
    }
}

#[cfg(test)]
pub(crate) fn connect_bundle_probe(
    address: &str,
    profile: u128,
) -> io::Result<Option<Arc<crate::server::client_bundle::ClientBundle>>> {
    let network = Network::connect(address, 1, profile)?;
    let Incoming::Message(first) = network
        .incoming
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
    else {
        panic!("connection closed before Welcome");
    };
    assert!(matches!(*first, ServerMessage::Welcome { .. }));
    Ok(network._bundle.clone())
}

#[cfg(test)]
pub(crate) fn connect_ui_probe(
    address: &str,
    profile: u128,
    inspect: impl FnOnce(&crate::server::client_bundle::ClientBundle, &mut crate::ui::authored::Session),
) -> io::Result<()> {
    let network = Network::connect(address, 1, profile)?;
    let mut session = network.package_ui().expect("package UI");
    inspect(network._bundle.as_ref().unwrap(), &mut session);
    Ok(())
}

#[cfg(test)]
pub(crate) fn connect_catalog_probe(address: &str, profile: u128) -> io::Result<Arc<Catalog>> {
    Ok(Network::connect(address, 1, profile)?.catalog)
}

#[cfg(test)]
pub(crate) fn connect_inventory_probe(
    address: &str,
    profile: u128,
    expected: crate::content::ItemId,
) -> io::Result<Arc<Catalog>> {
    let network = Network::connect(address, 1, profile)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let incoming = network
            .incoming
            .recv_timeout(remaining)
            .expect("inventory deadline");
        match incoming {
            Incoming::Message(message) => {
                if let ServerMessage::Inventory { slots, .. } = *message {
                    assert!(
                        slots
                            .iter()
                            .flatten()
                            .any(|stack| stack.item == expected && stack.count == 128)
                    );
                    return Ok(network.catalog);
                }
            }
            Incoming::Closed(error) => panic!("closed before inventory: {error}"),
        }
        assert!(std::time::Instant::now() < deadline, "inventory deadline");
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
    pub(super) jobs: super::mesh_queue::Sender,
    pub(super) urgent_jobs: super::mesh_queue::Sender,
    pub(super) results: Receiver<MesherResult>,
    revisions: Arc<Mutex<HashMap<ChunkKey, u64>>>,
}

pub(super) struct MesherResult {
    pub(super) mesh: ChunkMesh,
    /// Interior light samples are compact (~20 KiB/chunk) and let remote
    /// avatars share the exact worker-built light field with terrain.
    pub(super) lighting: Box<[LightSample]>,
}

pub(super) struct MesherJob {
    pub(super) chunk: Arc<Chunk>,
    pub(super) known: HashMap<ChunkKey, Arc<Chunk>>,
    pub(super) catalog: Arc<Catalog>,
    pub(super) seed: u64,
    pub(super) revision: u64,
    pub(super) bounced_gi: bool,
}

impl Mesher {
    /// Invalidate queued work before it consumes a lighting/mesh worker. The
    /// window thread's result/upload checks still guard races with active jobs.
    pub(super) fn invalidate(&self, key: ChunkKey, revision: Option<u64>) {
        self.jobs.invalidate(key);
        let mut revisions = self.revisions.lock().unwrap();
        if let Some(revision) = revision {
            revisions.insert(key, revision);
        } else {
            revisions.remove(&key);
        }
    }

    pub(super) fn new() -> Self {
        let (jobs, urgent_jobs, receiver) = super::mesh_queue::channel();
        let (results_tx, results) = mpsc::sync_channel(64);
        let revisions = Arc::new(Mutex::new(HashMap::new()));
        for _ in 0..2 {
            let receiver = receiver.clone();
            let results_tx = results_tx.clone();
            let revisions = Arc::clone(&revisions);
            thread::spawn(move || {
                while let Some(job) = receiver.recv() {
                    let current =
                        || revisions.lock().unwrap().get(&job.chunk.key) == Some(&job.revision);
                    if !current() {
                        continue;
                    }
                    super::trace::event(format_args!(
                        "start {:?} rev={} bounced={}",
                        job.chunk.key, job.revision, job.bounced_gi
                    ));
                    let light = if job.bounced_gi {
                        LightField::build_with_bounce_and_catalog(
                            job.chunk.key,
                            &job.known,
                            job.seed,
                            true,
                            &job.catalog,
                        )
                    } else {
                        LightField::build_with_catalog(
                            job.chunk.key,
                            &job.known,
                            job.seed,
                            &job.catalog,
                        )
                    };
                    // A new edit can arrive during propagation. Do not spend
                    // additional time meshing an already obsolete light field.
                    if !current() {
                        continue;
                    }
                    let mesh = render::mesh_chunk_lit_with_catalog(
                        &job.chunk,
                        &light,
                        job.revision,
                        &job.catalog,
                    );
                    let mut lighting = Vec::with_capacity(crate::world::CHUNK_VOLUME);
                    for y in 0..crate::world::CHUNK_SIZE {
                        for z in 0..crate::world::CHUNK_SIZE {
                            for x in 0..crate::world::CHUNK_SIZE {
                                // Zero face offset samples the interior voxel.
                                lighting.push(light.face([x, y, z], 1, 0));
                            }
                        }
                    }
                    if !current() {
                        continue;
                    }
                    super::trace::event(format_args!(
                        "ready {:?} rev={}",
                        job.chunk.key, job.revision
                    ));
                    if results_tx
                        .send(MesherResult {
                            mesh,
                            lighting: lighting.into_boxed_slice(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        Self {
            jobs,
            urgent_jobs,
            results,
            revisions,
        }
    }
}

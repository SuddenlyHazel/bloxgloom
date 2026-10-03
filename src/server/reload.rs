//! Explicit, bounded development reloads. Discovery and transport encoding stay
//! off the coordinator; only a validated immutable revision is published here.
use super::*;
use crate::content::Catalog;
use std::path::PathBuf;
use std::sync::{RwLock, mpsc};

pub(super) struct Development {
    root: PathBuf,
    base: ServerStartup,
    original: Arc<script::package::PackageSnapshot>,
    contract: startup::reload::Contract,
}

impl Development {
    pub(super) fn new(
        root: PathBuf,
        base: ServerStartup,
        declarations: &script::startup::Declarations,
        startup: &ServerStartup,
    ) -> io::Result<Self> {
        Ok(Self {
            root,
            base,
            original: Arc::clone(&declarations.snapshot),
            contract: startup.reload_contract()?,
        })
    }

    fn prepare(&self, catalog: Arc<Catalog>) -> io::Result<Prepared> {
        let snapshot = self
            .original
            .replacement(&self.root)
            .map_err(io::Error::other)?;
        let declarations = script::startup::Declarations::from_snapshot(Arc::clone(&snapshot))?;
        let candidate = self.base.clone().with_declarations(&declarations)?;
        if candidate.reload_contract()? != self.contract {
            return Err(io::Error::other(
                "restart required: frozen content, state schema, system plan or initial state changed",
            ));
        }
        let bundle = declarations.client_bundle;
        // Exercise the same bounded client startup used by native joins before
        // committing a revision that would strand every connected client.
        crate::client::prepare_package_startup(Arc::clone(&bundle))?;
        let content = net::ContentHandshake::with_bundle(catalog, Some(&bundle))?;
        Ok(Prepared {
            snapshot,
            bundle,
            content,
        })
    }
}

struct Prepared {
    snapshot: Arc<script::package::PackageSnapshot>,
    bundle: Arc<client_bundle::ClientBundle>,
    content: Arc<net::ContentHandshake>,
}

pub(super) struct HandshakeRevision {
    pub(super) revision: u64,
    pub(super) content: Arc<net::ContentHandshake>,
}

pub(super) struct Manager {
    development: Arc<Development>,
    pub(super) content: Arc<RwLock<Option<HandshakeRevision>>>,
    pending: Option<mpsc::Receiver<io::Result<Prepared>>>,
    ready: Option<Prepared>,
    requester: Option<u64>,
    pub(super) revision: u64,
}

impl Manager {
    pub(super) fn new(development: Arc<Development>) -> Self {
        Self {
            development,
            content: Arc::default(),
            pending: None,
            ready: None,
            requester: None,
            revision: 0,
        }
    }
}

fn status(state: &State, id: u64, text: impl Into<String>) {
    if let Some(client) = state.clients.get(&id) {
        client.enqueue(ServerMessage::PackageReload {
            reconnect: false,
            text: text.into().chars().take(1024).collect(),
        });
    }
}

pub(super) fn request(state: &mut State, id: u64) {
    if state
        .clients
        .get(&id)
        .is_none_or(|c| Some(c.profile) != state.admin_profile)
    {
        status(
            state,
            id,
            "Package reload requires the server admin profile",
        );
        return;
    }
    let Some(manager) = &mut state.reload else {
        status(
            state,
            id,
            "Package reload requires local-packages development startup",
        );
        return;
    };
    if manager.pending.is_some() || manager.ready.is_some() {
        status(state, id, "Package reload already in progress");
        return;
    }
    let development = Arc::clone(&manager.development);
    let catalog = state.world.catalog_arc();
    let (sender, receiver) = mpsc::sync_channel(1);
    match thread::Builder::new()
        .name("package-reload".into())
        .spawn(move || {
            let _ = sender.send(development.prepare(catalog));
        }) {
        Ok(_) => {
            manager.pending = Some(receiver);
            manager.requester = Some(id);
            status(state, id, "Validating package reload");
        }
        Err(error) => status(state, id, format!("Package reload failed: {error}")),
    }
}

pub(super) fn poll(state: &mut State) {
    let Some(manager) = &mut state.reload else {
        return;
    };
    if let Some(receiver) = &manager.pending {
        let result = match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(io::Error::other("reload worker stopped")))
            }
        };
        if let Some(result) = result {
            manager.pending = None;
            match result {
                Ok(ready) => manager.ready = Some(ready),
                Err(error) => {
                    let id = manager.requester.take();
                    if let Some(id) = id {
                        status(state, id, format!("Package reload rejected: {error}"));
                    }
                    return;
                }
            }
        }
    }
    // No accepted WAL work may straddle the revision boundary. Owner jobs are
    // joined by the preceding phase barriers; generators always keep startup.
    if !state.durability.pending.is_empty() {
        return;
    }
    let Some(ready) = manager.ready.take() else {
        return;
    };
    manager.development.original.publish(ready.snapshot);
    *manager.content.write().unwrap_or_else(|e| e.into_inner()) = Some(HandshakeRevision {
        revision: manager.revision + 1,
        content: ready.content,
    });
    manager.revision += 1;
    state.client_bundle = Some(ready.bundle);
    manager.requester = None;
    let clients: Vec<_> = state.clients.keys().copied().collect();
    for id in clients {
        reconnect(state, id);
    }
    tracing::info!("development packages reloaded");
}

pub(super) fn reconnect(state: &mut State, id: u64) {
    if let Some(client) = state.clients.get(&id) {
        client.enqueue(ServerMessage::PackageReload {
            reconnect: true,
            text: "Packages reloaded; refreshing client resources".into(),
        });
    }
    state.remove_client(id);
}

#[cfg(test)]
mod tests;

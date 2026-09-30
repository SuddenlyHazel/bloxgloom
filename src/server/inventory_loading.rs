//! Bounded filesystem loaders shared by admission and offline profile services.
use crate::inventory::{Inventory, InventoryStore};
use std::{
    io,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};
struct InventoryLoadRequest {
    profile: u128,
    reply: SyncSender<io::Result<Inventory>>,
}

/// Bounded, fixed-size pool for the filesystem portion of the join handshake.
pub(in crate::server) struct InventoryWorkers {
    sender: Option<SyncSender<InventoryLoadRequest>>,
    workers: Vec<JoinHandle<()>>,
}

impl InventoryWorkers {
    #[cfg(test)]
    pub(in crate::server) fn disabled() -> Self {
        Self {
            sender: None,
            workers: Vec::new(),
        }
    }
    pub(in crate::server) fn new(
        store: InventoryStore,
        capacity: usize,
        width: usize,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(width);
        for index in 0..width {
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

    pub(in crate::server) fn request(
        &self,
        profile: u128,
    ) -> Result<Receiver<io::Result<Inventory>>, ()> {
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

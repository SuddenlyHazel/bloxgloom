//! Lazy, bounded offline inventory loads. The coordinator never waits on files.
use crate::{
    inventory::{Inventory, InventoryStore},
    server::inventory_loading::InventoryWorkers,
};
use bloxgloom_host_api::gameplay::Error;
use std::{
    collections::BTreeMap,
    io,
    sync::mpsc::{Receiver, TryRecvError},
};
const CAPACITY: usize = 256;
struct Entry {
    used: u64,
    value: Result<Inventory, String>,
}
pub(in crate::server) struct Cache {
    store: InventoryStore,
    workers: Option<InventoryWorkers>,
    pending: BTreeMap<u128, Receiver<io::Result<Inventory>>>,
    entries: BTreeMap<u128, Entry>,
    clock: u64,
}
impl Cache {
    pub(in crate::server) fn new(store: InventoryStore) -> Self {
        Self {
            store,
            workers: None,
            pending: BTreeMap::new(),
            entries: BTreeMap::new(),
            clock: 0,
        }
    }
    pub(in crate::server) fn get(
        &mut self,
        profile: u128,
        revision: Option<u64>,
    ) -> Result<Inventory, Error> {
        self.clock = self.clock.saturating_add(1);
        if self.entries.get(&profile).is_some_and(|entry| {
            entry
                .value
                .as_ref()
                .is_ok_and(|value| revision.is_some_and(|r| r != value.revision))
        }) {
            self.entries.remove(&profile);
        }
        if let Some(entry) = self.entries.get_mut(&profile) {
            entry.used = self.clock;
            return entry.value.clone().map_err(Error::Invalid);
        }
        if let Some(receiver) = self.pending.get(&profile) {
            match receiver.try_recv() {
                Ok(value) => {
                    self.pending.remove(&profile);
                    let value = value.map_err(|e| format!("offline inventory load failed: {e}"));
                    self.remember_result(profile, value);
                    // Recheck a result against commits made while the file job ran.
                    return self.get(profile, revision);
                }
                Err(TryRecvError::Disconnected) => {
                    self.pending.remove(&profile);
                    return Err(Error::Host("offline inventory worker stopped".into()));
                }
                Err(TryRecvError::Empty) => {
                    return Err(Error::Deferred("offline inventory loading".into()));
                }
            }
        }
        if self.pending.len() >= 64 {
            return Err(Error::Deferred("offline inventory load queue full".into()));
        }
        if self.workers.is_none() {
            self.workers = Some(
                InventoryWorkers::new(self.store.clone(), CAPACITY, 2)
                    .map_err(|e| Error::Host(e.to_string()))?,
            );
        }
        let receiver = self
            .workers
            .as_ref()
            .expect("loader installed")
            .request(profile)
            .map_err(|_| Error::Deferred("offline inventory load queue full".into()))?;
        self.pending.insert(profile, receiver);
        Err(Error::Deferred("offline inventory loading".into()))
    }
    fn remember_result(&mut self, profile: u128, value: Result<Inventory, String>) {
        if self.entries.len() >= CAPACITY
            && !self.entries.contains_key(&profile)
            && let Some(old) = self
                .entries
                .iter()
                .min_by_key(|(profile, entry)| (entry.used, **profile))
                .map(|(p, _)| *p)
        {
            self.entries.remove(&old);
        }
        self.entries.insert(
            profile,
            Entry {
                used: self.clock,
                value,
            },
        );
    }
    pub(in crate::server) fn remember(&mut self, profile: u128, inventory: Inventory) {
        if self.workers.is_none() {
            return;
        }
        self.pending.remove(&profile);
        self.remember_result(profile, Ok(inventory));
    }
}

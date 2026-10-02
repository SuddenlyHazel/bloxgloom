//! Bounded, connection-owned stack presentation. Render callers only consult
//! cached data and enqueue misses; all Luau work belongs to the worker thread.
use crate::{
    content::Catalog, inventory::Stack, items::ItemId, server::client_bundle::ClientBundle,
};
use bloxgloom_host_api::icon::ItemIcon;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, mpsc},
};
mod execution;
#[cfg(test)]
pub(crate) mod tests;

const MAX_ENTRIES: usize = 512;
const QUEUE: usize = 64;

#[derive(Clone, Debug)]
pub(crate) struct Visual {
    pub(crate) icon: Option<Arc<ItemIcon>>,
    /// Client-only multiplier for the ordinary dropped-item art.
    pub(crate) drop_scale: f32,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    item: u32,
    count: u16,
    version: u16,
    bytes: Vec<u8>,
}
impl Key {
    fn new(stack: &Stack) -> Self {
        Self {
            item: stack.item.0,
            count: stack.count,
            version: stack.components.as_ref().map_or(0, |p| p.version),
            bytes: stack
                .components
                .as_ref()
                .map_or_else(Vec::new, |p| p.bytes.to_vec()),
        }
    }
}
#[derive(Default)]
struct State {
    handlers: BTreeMap<ItemId, (String, String)>,
    sender: Option<mpsc::SyncSender<Key>>,
    entries: BTreeMap<Key, Entry>,
    clock: u64,
}
struct Entry {
    pending: bool,
    visual: Option<Arc<Visual>>,
    used: u64,
}
#[derive(Default)]
pub(crate) struct Cache(Mutex<State>);
impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StackVisualCache")
    }
}
impl Cache {
    pub(crate) fn bind(
        self: &Arc<Self>,
        bundle: Arc<ClientBundle>,
        handlers: &BTreeMap<String, String>,
        catalog: &Catalog,
    ) -> std::io::Result<()> {
        if handlers.is_empty() {
            return Ok(());
        }
        let mut resolved = BTreeMap::new();
        for (item, module) in handlers {
            let id = catalog.item_by_key(item).ok_or_else(|| {
                std::io::Error::other(format!("item visual references missing item {item}"))
            })?;
            resolved.insert(id, (item.clone(), module.clone()));
        }
        let (sender, receiver) = mpsc::sync_channel::<Key>(QUEUE);
        let weak = Arc::downgrade(self);
        let worker_handlers = resolved.clone();
        std::thread::Builder::new().name("client-item-visuals".into()).spawn(move || {
            while let Ok(key) = receiver.recv() {
                let (item, module) = &worker_handlers[&ItemId(key.item)];
                let visual = execution::run(Arc::clone(&bundle), module, item, &key);
                if let Err(error) = &visual { tracing::warn!(%module, %error, "item visual callback failed; using ordinary item art"); }
                let Some(cache) = weak.upgrade() else { break; };
                if let Ok(mut state) = cache.0.lock() && let Some(entry) = state.entries.get_mut(&key) {
                    entry.pending = false;
                    entry.visual = visual.ok().map(Arc::new);
                }
            }
        })?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| std::io::Error::other("item visual cache poisoned"))?;
        if state.sender.is_some() {
            return Err(std::io::Error::other("item visuals already installed"));
        }
        state.handlers = resolved;
        state.sender = Some(sender);
        Ok(())
    }
    pub(crate) fn visual(&self, stack: &Stack) -> Option<Arc<Visual>> {
        // The render thread must never wait behind worker publication.
        let Ok(mut state) = self.0.try_lock() else {
            return None;
        };
        if !state.handlers.contains_key(&stack.item)
            || !(1..=128).contains(&stack.count)
            || stack
                .components
                .as_ref()
                .is_some_and(|p| p.bytes.len() > 1024)
        {
            return None;
        }
        let key = Key::new(stack);
        state.clock = state.clock.wrapping_add(1);
        let clock = state.clock;
        if let Some(entry) = state.entries.get_mut(&key) {
            entry.used = clock;
            return entry.visual.clone();
        }
        if state.entries.len() == MAX_ENTRIES {
            let oldest = state
                .entries
                .iter()
                .filter(|(_, entry)| !entry.pending)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone());
            state.entries.remove(&oldest?);
        }
        if state.sender.as_ref()?.try_send(key.clone()).is_err() {
            return None;
        }
        state.entries.insert(
            key,
            Entry {
                pending: true,
                visual: None,
                used: clock,
            },
        );
        None
    }
}

//! O(1), immutable session-specific publication dependencies.
use crate::server::Client;
use crate::world::ChunkKey;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub(in crate::server) struct Capture {
    pub id: u64,
    pub profile: u128,
    pub sent: Arc<HashSet<ChunkKey>>,
    pub epochs: Arc<HashMap<ChunkKey, u64>>,
    pub blocks: Arc<HashMap<ChunkKey, u64>>,
    pub entities: Arc<HashMap<ChunkKey, u64>>,
}
impl Capture {
    pub(in crate::server) fn new(id: u64, client: &Client) -> Self {
        Self {
            id,
            profile: client.profile,
            sent: client.sent.capture(),
            epochs: client.sent_epochs.capture(),
            blocks: client.sent_block_versions.capture(),
            entities: client.sent_entity_revisions.capture(),
        }
    }
    pub(in crate::server) fn current(&self, client: &Client) -> bool {
        self.profile == client.profile
            && client.sent.matches(&self.sent)
            && client.sent_epochs.matches(&self.epochs)
            && client.sent_block_versions.matches(&self.blocks)
            && client.sent_entity_revisions.matches(&self.entities)
    }
}

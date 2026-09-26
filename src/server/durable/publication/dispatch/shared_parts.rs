//! One bounded fanout batch may reuse byte-identical transaction parts. Payload
//! identity comes from the enclosing single immutable CommitChanges; the key
//! includes every client-specific wire header. Never reused across effects.
use crate::protocol::{ServerMessage, WorldCommitPart};
use crate::server::outbound::{OUTBOUND_FRAME_CAPACITY, SharedMessage};
use crate::server::streaming::workers::WIDTH;
use crate::world::ChunkKey;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Weak};

#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct Identity {
    commit: u64,
    index: u16,
    count: u16,
    chunk: ChunkKey,
    epoch: u64,
    blocks: (u64, u64),
    entities: (u64, u64),
}

#[derive(Default)]
pub(super) struct SharedParts(Mutex<BTreeMap<Identity, Weak<SharedMessage>>>);

impl SharedParts {
    pub(super) fn frame(&self, part: WorldCommitPart) -> Arc<SharedMessage> {
        let key = Identity {
            commit: part.commit_id,
            index: part.part_index,
            count: part.part_count,
            chunk: part.key,
            epoch: part.epoch,
            blocks: (part.block_from, part.block_to),
            entities: (part.entity_from, part.entity_to),
        };
        let mut frames = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(frame) = frames.get(&key).and_then(Weak::upgrade) {
            return frame;
        }
        let frame = SharedMessage::new(ServerMessage::WorldCommitPart(part));
        if frames.len() < WIDTH * OUTBOUND_FRAME_CAPACITY {
            frames.insert(key, Arc::downgrade(&frame));
        }
        frame
    }
}

//! Lightweight revision/page probe for real TCP soak peers. The game client
//! performs full block/entity assembly; the load generator tracks only cursors.

use crate::protocol::{EntitySnapshotPage, WorldCommitPart, WorldSnapshotStart};
use crate::world::ChunkKey;
use std::collections::{BTreeMap, HashMap};
use std::io;

const MAX_PENDING_GROUPS: usize = 8;
const MAX_PENDING_SNAPSHOTS: usize = 8;

#[derive(Clone, Copy)]
struct Cursor {
    epoch: u64,
    block_revision: u64,
    entity_revision: u64,
}

struct PendingSnapshot {
    cursor: Cursor,
    page_count: u16,
    checksum: u64,
    seen: Vec<bool>,
}

struct PendingCommit {
    parts: Vec<Option<WorldCommitPart>>,
    count: usize,
}

#[derive(Default)]
pub(super) struct WorldProbe {
    cursors: HashMap<ChunkKey, Cursor>,
    snapshots: HashMap<ChunkKey, PendingSnapshot>,
    commits: BTreeMap<u64, PendingCommit>,
    last_commit: u64,
}

#[derive(Default)]
pub(super) struct WorldUpdate {
    pub(super) snapshots: u64,
    pub(super) block_changes: u64,
    pub(super) gaps: u64,
    pub(super) regressions: u64,
    pub(super) resync: Vec<ChunkKey>,
}

impl WorldProbe {
    pub(super) fn start(&mut self, start: WorldSnapshotStart) -> io::Result<WorldUpdate> {
        let key = start.chunk.key;
        let mut update = WorldUpdate::default();
        if self
            .cursors
            .get(&key)
            .is_some_and(|old| start.epoch <= old.epoch)
        {
            return Ok(update);
        }
        if self.snapshots.len() >= MAX_PENDING_SNAPSHOTS && !self.snapshots.contains_key(&key) {
            return Err(io::Error::other(
                "TCP client snapshot assembly bound exceeded",
            ));
        }
        let cursor = Cursor {
            epoch: start.epoch,
            block_revision: start.chunk.version,
            entity_revision: start.entity_revision,
        };
        if let Some(old) = self.cursors.get(&key)
            && cursor.block_revision < old.block_revision
        {
            update.regressions += 1;
        }
        if start.entity_page_count == 0 {
            self.snapshots.remove(&key);
            self.cursors.insert(key, cursor);
            update.snapshots = 1;
        } else {
            self.snapshots.insert(
                key,
                PendingSnapshot {
                    cursor,
                    page_count: start.entity_page_count,
                    checksum: start.checksum,
                    seen: vec![false; usize::from(start.entity_page_count)],
                },
            );
        }
        Ok(update)
    }

    pub(super) fn page(&mut self, page: EntitySnapshotPage) -> io::Result<WorldUpdate> {
        let update = WorldUpdate::default();
        if self
            .cursors
            .get(&page.key)
            .is_some_and(|old| page.epoch <= old.epoch)
        {
            return Ok(update);
        }
        let pending = self
            .snapshots
            .get_mut(&page.key)
            .ok_or_else(|| io::Error::other("TCP client received page without snapshot start"))?;
        if page.epoch != pending.cursor.epoch
            || page.entity_revision != pending.cursor.entity_revision
            || page.page_count != pending.page_count
            || page.checksum != pending.checksum
        {
            return Err(io::Error::other(
                "TCP client received mismatched snapshot page",
            ));
        }
        let seen = pending
            .seen
            .get_mut(usize::from(page.page_index))
            .ok_or_else(|| io::Error::other("TCP client page index out of range"))?;
        if *seen {
            return Err(io::Error::other(
                "TCP client received duplicate snapshot page",
            ));
        }
        *seen = true;
        if pending.seen.iter().all(|seen| *seen) {
            let cursor = pending.cursor;
            self.snapshots.remove(&page.key);
            self.cursors.insert(page.key, cursor);
            return Ok(WorldUpdate {
                snapshots: 1,
                ..WorldUpdate::default()
            });
        }
        Ok(update)
    }

    pub(super) fn commit(&mut self, part: WorldCommitPart) -> io::Result<WorldUpdate> {
        if part.commit_id <= self.last_commit {
            return Ok(WorldUpdate::default());
        }
        if self.commits.len() >= MAX_PENDING_GROUPS && !self.commits.contains_key(&part.commit_id) {
            return Err(io::Error::other(
                "TCP client commit assembly bound exceeded",
            ));
        }
        let pending = self
            .commits
            .entry(part.commit_id)
            .or_insert_with(|| PendingCommit {
                parts: vec![None; usize::from(part.part_count)],
                count: 0,
            });
        if pending.parts.len() != usize::from(part.part_count) {
            return Err(io::Error::other("TCP client commit part count changed"));
        }
        let slot = &mut pending.parts[usize::from(part.part_index)];
        if let Some(existing) = slot {
            if existing != &part {
                return Err(io::Error::other(
                    "TCP client conflicting duplicate commit part",
                ));
            }
            return Ok(WorldUpdate::default());
        }
        *slot = Some(part);
        pending.count += 1;
        self.finish_commits()
    }

    fn finish_commits(&mut self) -> io::Result<WorldUpdate> {
        let mut update = WorldUpdate::default();
        while let Some((&id, pending)) = self.commits.first_key_value() {
            if pending.count != pending.parts.len() {
                break;
            }
            let pending = self.commits.remove(&id).expect("first key exists");
            let parts: Vec<_> = pending.parts.into_iter().map(Option::unwrap).collect();
            let mismatch: Vec<_> = parts
                .iter()
                .filter(|part| {
                    self.cursors.get(&part.key).is_none_or(|cursor| {
                        cursor.epoch != part.epoch
                            || cursor.block_revision != part.block_from
                            || cursor.entity_revision != part.entity_from
                    })
                })
                .map(|part| part.key)
                .collect();
            if !mismatch.is_empty() {
                update.gaps += 1;
                update.resync.extend(mismatch);
                self.commits.clear();
                return Ok(update);
            }
            for part in parts {
                let cursor = self.cursors.get_mut(&part.key).expect("validated cursor");
                cursor.block_revision = part.block_to;
                cursor.entity_revision = part.entity_to;
                update.block_changes += part.blocks.len() as u64;
            }
            self.last_commit = id;
        }
        Ok(update)
    }
}

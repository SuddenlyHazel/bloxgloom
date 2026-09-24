//! Isolated performance-save bootstrap through normal fire WAL transactions.
//!
//! The test fixture provides a dense initial frontier, but the measured ticks
//! use the registered gameplay handlers and ordinary WAL receipt application.

use super::*;

impl FireRuntime {
    /// Prepare at most one fresh owner per durable cursor lane. The caller
    /// stages this wave and waits for its synced receipts before requesting
    /// another, so every cursor `before` value is authoritative.
    pub(in crate::server) fn prepare_benchmark_frontier_wave(
        &self,
        owners: &BTreeMap<ChunkKey, Vec<u16>>,
        tick: TickId,
    ) -> io::Result<FireWave> {
        if tick.get() == 0 {
            return Err(invalid("fire fixture tick must be nonzero"));
        }
        let mut lanes = BTreeSet::new();
        let mut transactions = Vec::new();
        for (&owner, cells) in owners {
            if self.frontiers.contains_key(&owner) || self.inflight_frontiers.contains(&owner) {
                return Err(invalid("fire fixture owner already has a frontier"));
            }
            let lane = owner_lane(owner);
            if !lanes.insert(lane) {
                continue;
            }
            let mut frontier = FireFrontier::default();
            for &cell in cells {
                frontier.insert(cell, tick.get())?;
            }
            if frontier.is_empty() {
                return Err(invalid("fire fixture frontier is empty"));
            }
            let cursor_before = self.cursors[lane];
            let cursor_after = FireCursor {
                last_owner: Some(owner),
                last_source: None,
                last_tick: tick.get(),
            };
            transactions.push(FireTransaction {
                owner,
                burns: Vec::new(),
                changed_cells: Vec::new(),
                world_edit: None,
                changes: vec![
                    Change::new(frontier_key(owner), Vec::new(), frontier.encode()),
                    Change::new(
                        cursor_key(lane),
                        cursor_before.encode(),
                        cursor_after.encode(),
                    ),
                ],
                emitted_effects: 0,
                delivered_effects: 0,
                frontier_after: frontier,
                mailboxes: Vec::new(),
                cursor_lane: lane,
                cursor_after,
            });
        }
        Ok(FireWave {
            transactions,
            missing_chunks: Vec::new(),
            deferred_owners: 0,
            timings: FireWaveTimings::default(),
        })
    }

    /// Stable, off-tick digest used to compare the drained live frontier with
    /// an independent restart from the isolated benchmark save.
    pub(in crate::server) fn benchmark_state_fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        fn add(hash: &mut u64, bytes: &[u8]) {
            for &byte in bytes {
                *hash ^= u64::from(byte);
                *hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
        for (&owner, frontier) in &self.frontiers {
            add(&mut hash, &[0]);
            add(&mut hash, &key_bytes(owner));
            add(&mut hash, &frontier.encode());
        }
        for (&(destination, source), pending) in &self.pending {
            add(&mut hash, &[1]);
            add(&mut hash, &key_bytes(destination));
            add(&mut hash, &key_bytes(source));
            add(&mut hash, &pending.encode());
        }
        for (lane, cursor) in self.cursors.iter().enumerate() {
            add(&mut hash, &[2, lane as u8]);
            add(&mut hash, &cursor.encode());
        }
        hash
    }
}

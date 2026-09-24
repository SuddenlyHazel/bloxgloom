//! Parallel construction of exact WAL after-values from validated owner patches.
//!
//! Each closure owns its captured frontier/mailbox snapshots. Collection is in
//! the original owner order, independent of worker completion order.

use super::*;
use crate::server::parallel::JobOutcome;

struct SourceEncodeInput {
    patch: FireOwnerPatch,
    before_frontier: Arc<FireFrontier>,
    before_mailboxes: BTreeMap<ChunkKey, Arc<FirePending>>,
    cursor_before: FireCursor,
}

impl FireRuntime {
    pub(super) fn encode_source_patches(
        &mut self,
        patches: Vec<FireOwnerPatch>,
        tick: TickId,
        owner_batch: BatchId,
    ) -> io::Result<(Vec<FireTransaction>, usize)> {
        let attempted = patches.len();
        let mut order = Vec::with_capacity(attempted);
        let mut submitted = 0usize;
        let mut submit_error = None;
        for patch in patches {
            if patch.consumed.is_empty()
                || patch.mailbox_after.keys().any(|&destination| {
                    self.inflight_mailboxes
                        .contains(&(destination, patch.owner))
                })
            {
                continue;
            }
            let owner = patch.owner;
            let version = patch.expected_chunk_version;
            let before_frontier = Arc::clone(
                self.frontiers
                    .get(&owner)
                    .ok_or_else(|| invalid("fire source frontier disappeared"))?,
            );
            let before_mailboxes = patch
                .mailbox_after
                .keys()
                .filter_map(|&destination| {
                    self.pending
                        .get(&(destination, owner))
                        .map(|mailbox| (destination, Arc::clone(mailbox)))
                })
                .collect();
            let input = SourceEncodeInput {
                patch,
                before_frontier,
                before_mailboxes,
                cursor_before: self.cursors[owner_lane(owner)],
            };
            let key = JobKey::new(owner_batch, owner, 0, version);
            if let Err(error) = self
                .encode_executor
                .try_submit(key, move |_| source_transaction(input, tick.get()))
            {
                submit_error = Some(format!("fire encode worker admission: {error:?}"));
                break;
            }
            order.push(owner);
            submitted += 1;
        }
        if submitted == 0 {
            return submit_error.map_or_else(
                || Ok((Vec::new(), attempted)),
                |error| Err(io::Error::other(error)),
            );
        }
        let results = self
            .encode_executor
            .barrier(owner_batch)
            .map_err(|error| io::Error::other(format!("fire encode barrier: {error:?}")))?;
        if let Some(error) = submit_error {
            return Err(io::Error::other(error));
        }
        let mut encoded = BTreeMap::new();
        for owner in results.owners {
            let OwnerKey::Chunk(chunk) = owner.owner else {
                return Err(invalid("fire encode result has non-chunk owner"));
            };
            if owner.jobs.len() != 1 {
                return Err(invalid("fire encode result has wrong job count"));
            }
            let job = owner.jobs.into_iter().next().unwrap();
            if job.key.owner != owner.owner {
                return Err(invalid("fire encode result owner mismatch"));
            }
            let value = match job.outcome {
                JobOutcome::Completed(value) => value,
                JobOutcome::Failed(error) => return Err(error),
                JobOutcome::Panicked(error) => {
                    return Err(io::Error::other(format!(
                        "fire encode worker panicked: {error}"
                    )));
                }
                JobOutcome::Cancelled | JobOutcome::Stale => {
                    return Err(invalid("fire encode job cancelled or stale"));
                }
            };
            if encoded.insert(chunk, value).is_some() {
                return Err(invalid("duplicate fire encode owner"));
            }
        }
        if encoded.len() != submitted {
            return Err(invalid("fire encode barrier lost owner"));
        }
        let mut transactions = Vec::with_capacity(submitted);
        for owner in order {
            if let Some(transaction) = encoded
                .remove(&owner)
                .ok_or_else(|| invalid("fire encode owner result missing"))?
            {
                transactions.push(transaction);
            }
        }
        let deferred = attempted - transactions.len();
        Ok((transactions, deferred))
    }
}

fn source_transaction(input: SourceEncodeInput, tick: u64) -> io::Result<Option<FireTransaction>> {
    let SourceEncodeInput {
        patch,
        before_frontier,
        before_mailboxes,
        cursor_before,
    } = input;
    let source = patch.owner;
    let lane = owner_lane(source);
    let mut changes = vec![Change::new(
        frontier_key(source),
        before_frontier.encode(),
        patch.frontier_after.encode(),
    )];
    if let Some(edit) = &patch.world_edit {
        changes.push(Change::new(
            StateKey::new("bloxgloom:chunk_snapshot", key_bytes(source).to_vec()),
            edit.before_snapshot.clone(),
            edit.after_snapshot.clone(),
        ));
    }
    let mut mailboxes = Vec::with_capacity(patch.mailbox_after.len());
    for (destination, after) in patch.mailbox_after {
        let before = before_mailboxes
            .get(&destination)
            .map_or_else(Vec::new, |mailbox| mailbox.encode());
        let encoded_after = after.encode();
        if before == encoded_after {
            continue;
        }
        changes.push(Change::new(
            mailbox_key(destination, source),
            before,
            encoded_after,
        ));
        mailboxes.push(MailboxUpdate {
            destination,
            source,
            after,
        });
    }
    let cursor_after = FireCursor {
        last_owner: Some(source),
        last_source: None,
        last_tick: tick,
    };
    changes.push(Change::new(
        cursor_key(lane),
        cursor_before.encode(),
        cursor_after.encode(),
    ));
    // Retain the source if a valid atomic transition would exceed the strict
    // WAL record bound. The owner will be retried without changing state.
    let estimated = changes.iter().fold(0usize, |sum, change| {
        sum.saturating_add(change.key.domain.len())
            .saturating_add(change.key.bytes.len())
            .saturating_add(change.before.len())
            .saturating_add(change.after.len())
            .saturating_add(64)
    });
    if estimated > 900_000 {
        return Ok(None);
    }
    let transaction = FireTransaction {
        owner: source,
        burns: patch.burns,
        changed_cells: patch.changed_cells,
        world_edit: patch.world_edit,
        changes,
        emitted_effects: patch.effect_count,
        delivered_effects: 0,
        frontier_after: patch.frontier_after,
        mailboxes,
        cursor_lane: lane,
        cursor_after,
    };
    validate_transaction(&transaction)?;
    Ok(Some(transaction))
}

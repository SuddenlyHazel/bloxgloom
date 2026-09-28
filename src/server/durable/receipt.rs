//! One ordered receipt/apply gate for every accepted durable producer.
use super::*;
use crate::server::State;
use crate::server::fire::FireTransaction;
use crate::server::metrics::LatencyEvent;

/// Explicit coordinator wait boundary, never a speculative simulation step.
/// `Through` includes every earlier admission, regardless of producer. A
/// later ready receipt cannot pass an earlier pending one. `AllStaged` fixes
/// its frontier when entered; applying a payload never admits more work.
#[derive(Clone, Copy, Debug)]
pub(in crate::server) enum CommitBarrier {
    Through(u128),
    AllStaged,
}

#[derive(Default)]
pub(in crate::server) struct CommitProgress {
    pub commits: usize,
    pub owner_writes: usize,
}

type ReadyFire = (Vec<StateKey>, FireTransaction);

fn flush_ready_fire(state: &mut State, ready: &mut Vec<ReadyFire>) -> io::Result<()> {
    if ready.is_empty() {
        return Ok(());
    }
    let (keys, transactions): (Vec<_>, Vec<_>) = std::mem::take(ready).into_iter().unzip();
    super::fire::apply_synced_batch(state, transactions)?;
    for keys in keys {
        state.durability.release_reservations(keys, Vec::new());
    }
    Ok(())
}

/// Nonblocking maintenance uses exactly the same ordered apply path as a
/// logical-step barrier; receipt availability affects latency, not ordering.
pub(in crate::server) fn poll_journal_receipts(state: &mut State) -> io::Result<()> {
    advance_receipts(state, None).map(|_| ())
}

/// Completes a logical durable boundary on the coordinator. Fsync may stretch
/// its wall-clock duration. Network I/O continues on the independent reactor;
/// no worker, next physics step or publication sees unconfirmed state.
pub(in crate::server) fn complete_barrier(
    state: &mut State,
    barrier: CommitBarrier,
) -> io::Result<CommitProgress> {
    let frontier = match barrier {
        CommitBarrier::Through(id) => Some(id),
        CommitBarrier::AllStaged => state.durability.pending.last().map(|commit| commit.id),
    };
    advance_receipts(state, frontier)
}

pub(super) fn drain_staged_receipts(state: &mut State) -> io::Result<()> {
    complete_barrier(state, CommitBarrier::AllStaged).map(|_| ())
}

fn advance_receipts(state: &mut State, wait_through: Option<u128>) -> io::Result<CommitProgress> {
    if state.durability.failed {
        return Err(io::Error::other(
            "durable subsystem failed; restart required",
        ));
    }
    let mut fire = Vec::new();
    let mut progress = CommitProgress::default();
    let result = (|| {
        while let Some(front) = state.durability.pending.first() {
            if wait_through.is_some_and(|id| front.id > id) {
                break;
            }
            let receipt = if wait_through.is_some() {
                front
                    .receiver
                    .recv()
                    .map_err(|_| TryRecvError::Disconnected)
            } else {
                front.receiver.try_recv()
            };
            let receipt = match receipt {
                Ok(Ok(receipt)) => receipt,
                Err(TryRecvError::Empty) => break,
                error => {
                    state.metrics.record_latency(
                        LatencyEvent::DurableWalReceipt,
                        front.submitted_at.elapsed(),
                    );
                    // A confirmed prefix stays applied even if its successor
                    // fails. Retain the failed record and suffix quarantined
                    // with their reservations/permits until state destruction.
                    // They cannot be retried or applied; WAL replay is authority.
                    flush_ready_fire(state, &mut fire)?;
                    return Err(io::Error::other(match error {
                        Ok(Err(error)) => format!("durable WAL write failed: {error}"),
                        _ => "durable WAL worker disconnected".into(),
                    }));
                }
            };
            let commit = state.durability.pending.remove(0);
            state.metrics.record_latency(
                LatencyEvent::DurableWalReceipt,
                commit.submitted_at.elapsed(),
            );
            match commit.payload {
                PendingPayload::Fire(transaction) => fire.push((commit.keys, transaction)),
                payload => {
                    flush_ready_fire(state, &mut fire)?;
                    match payload {
                        PendingPayload::FireAction(transaction, action) => {
                            state
                                .fire
                                .validate_synced_batch(std::slice::from_ref(&transaction))?;
                            super::publication::apply_committed_action(
                                state,
                                action,
                                commit.entity_permit,
                            )?;
                            super::publication::publish_committed_fire_after_world(
                                state,
                                transaction,
                            )?;
                        }
                        PendingPayload::Action(action) => {
                            super::publication::apply_committed_action(
                                state,
                                action,
                                commit.entity_permit,
                            )?;
                        }
                        PendingPayload::Owner(mut owner) => {
                            if let Some(world) = owner.world_action.take() {
                                super::publication::apply_committed_owner_world(
                                    state,
                                    world.0,
                                    commit.entity_permit,
                                )?;
                            }
                            progress.owner_writes += state
                                .system_runtime
                                .apply_receipted_owner_wave(owner, receipt)?;
                        }
                        PendingPayload::Fire(_) => unreachable!(),
                    }
                    state
                        .durability
                        .release_reservations(commit.keys, commit.shared_read_keys);
                }
            }
            progress.commits += 1;
        }
        flush_ready_fire(state, &mut fire)
    })();
    if let Err(error) = result {
        state.durability.failed = true;
        return Err(io::Error::other(format!(
            "durable commit failed; restart required: {error}"
        )));
    }
    Ok(progress)
}

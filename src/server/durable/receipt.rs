//! Poll write-ahead log receipts and apply only successfully synced actions.

use super::*;
use crate::server::State;
use crate::server::fire::FireTransaction;
use crate::server::metrics::LatencyEvent;

/// Fire keys remain reserved until the complete contiguous owner batch has
/// applied. A ready WAL receipt alone is never enough to release its owner.
type ReadyFire = (Vec<StateKey>, FireTransaction);

fn flush_ready_fire(state: &mut State, ready: &mut Vec<ReadyFire>) -> io::Result<()> {
    if ready.is_empty() {
        return Ok(());
    }
    let ready = std::mem::take(ready);
    let (keys, transactions): (Vec<_>, Vec<_>) = ready.into_iter().unzip();
    super::fire::apply_synced_batch(state, transactions)?;
    for record_keys in keys {
        for key in record_keys {
            state.durability.reserved.remove(&key);
        }
    }
    Ok(())
}

fn failed_apply(state: &mut State, error: io::Error) -> io::Error {
    state.durability.failed = true;
    io::Error::other(format!(
        "WAL committed but in-memory apply failed; restarting is required: {error}"
    ))
}

pub(in crate::server) fn poll_journal_receipts(state: &mut State) -> io::Result<()> {
    let pending = std::mem::take(&mut state.durability.pending);
    let mut remaining = pending.into_iter();
    let mut ready_fire = Vec::<ReadyFire>::new();
    while let Some(commit) = remaining.next() {
        match commit.receiver.try_recv() {
            Ok(Ok(_receipt)) => {
                if let Err(error) = apply_confirmed(state, commit, &mut ready_fire) {
                    state.durability.pending.extend(remaining);
                    return Err(failed_apply(state, error));
                }
            }
            Ok(Err(error)) => {
                if let Err(apply_error) = flush_ready_fire(state, &mut ready_fire) {
                    state.durability.pending.extend(remaining);
                    return Err(failed_apply(state, apply_error));
                }
                state.metrics.record_latency(
                    LatencyEvent::DurableWalReceipt,
                    commit.submitted_at.elapsed(),
                );
                state.durability.failed = true;
                state.durability.pending.extend(remaining);
                return Err(io::Error::other(format!(
                    "durable WAL write failed: {error}"
                )));
            }
            Err(TryRecvError::Empty) => {
                if let Err(error) = flush_ready_fire(state, &mut ready_fire) {
                    state.durability.pending.push(commit);
                    state.durability.pending.extend(remaining);
                    return Err(failed_apply(state, error));
                }
                // A later receipt cannot pass this WAL record, even if it is
                // ready. Its action or fire wave may depend on this one.
                state.durability.pending.push(commit);
                state.durability.pending.extend(remaining);
                return Ok(());
            }
            Err(TryRecvError::Disconnected) => {
                if let Err(error) = flush_ready_fire(state, &mut ready_fire) {
                    state.durability.pending.extend(remaining);
                    return Err(failed_apply(state, error));
                }
                state.durability.failed = true;
                state.durability.pending.extend(remaining);
                return Err(io::Error::other("durable WAL worker disconnected"));
            }
        }
    }
    if let Err(error) = flush_ready_fire(state, &mut ready_fire) {
        return Err(failed_apply(state, error));
    }
    Ok(())
}

/// Applies one WAL-confirmed commit in WAL order: fire waves join the
/// contiguous owner batch, gameplay actions apply through publication and
/// release their keys. Receipt latency is recorded for the confirmed write.
/// The commit is consumed; on apply failure the caller fails closed.
fn apply_confirmed(
    state: &mut State,
    commit: PendingCommit,
    ready_fire: &mut Vec<ReadyFire>,
) -> io::Result<()> {
    state.metrics.record_latency(
        LatencyEvent::DurableWalReceipt,
        commit.submitted_at.elapsed(),
    );
    match commit.payload {
        PendingPayload::Fire(transaction) => {
            ready_fire.push((commit.keys, transaction));
            Ok(())
        }
        PendingPayload::Action(action) => {
            flush_ready_fire(state, ready_fire)?;
            super::publication::apply_committed_action(state, action, commit.entity_permit)?;
            for key in commit.keys {
                state.durability.reserved.remove(&key);
            }
            Ok(())
        }
    }
}

/// Blocking in-order apply of every staged WAL receipt.
///
/// The durable phase calls this after staging server-scheduled entity work
/// (motion, expiry): the whole tick's motion is committed and applied before
/// the tick ends, so the state at tick N+1 is a pure function of the state
/// at tick N. Journal (fsync) latency stretches this tick's wall time but
/// can never change the trajectory: receipts are consumed in WAL order, a
/// later receipt still cannot pass an earlier one, unconfirmed work is never
/// applied, and a failed write still fails closed with the unapplied suffix
/// left queued behind `failed`.
pub(super) fn drain_staged_receipts(state: &mut State) -> io::Result<()> {
    let mut ready_fire = Vec::<ReadyFire>::new();
    while !state.durability.pending.is_empty() {
        let commit = state.durability.pending.remove(0);
        match commit.receiver.recv() {
            Ok(Ok(_receipt)) => {
                if let Err(error) = apply_confirmed(state, commit, &mut ready_fire) {
                    return Err(failed_apply(state, error));
                }
            }
            Ok(Err(error)) => {
                if let Err(apply_error) = flush_ready_fire(state, &mut ready_fire) {
                    return Err(failed_apply(state, apply_error));
                }
                state.metrics.record_latency(
                    LatencyEvent::DurableWalReceipt,
                    commit.submitted_at.elapsed(),
                );
                state.durability.failed = true;
                return Err(io::Error::other(format!(
                    "durable WAL write failed: {error}"
                )));
            }
            Err(_) => {
                if let Err(apply_error) = flush_ready_fire(state, &mut ready_fire) {
                    return Err(failed_apply(state, apply_error));
                }
                state.durability.failed = true;
                return Err(io::Error::other("durable WAL worker disconnected"));
            }
        }
    }
    if let Err(error) = flush_ready_fire(state, &mut ready_fire) {
        return Err(failed_apply(state, error));
    }
    Ok(())
}

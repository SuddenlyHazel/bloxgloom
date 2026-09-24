//! Poll write-ahead log receipts and apply only successfully synced actions.

use super::*;
use crate::server::State;
use crate::server::metrics::LatencyEvent;

pub(super) fn poll_journal_receipts(state: &mut State) -> io::Result<()> {
    let pending = std::mem::take(&mut state.durability.pending);
    let mut waiting = Vec::new();
    let mut remaining = pending.into_iter();
    while let Some(commit) = remaining.next() {
        match commit.receiver.try_recv() {
            Ok(Ok(_receipt)) => {
                state.metrics.record_latency(
                    LatencyEvent::DurableWalReceipt,
                    commit.submitted_at.elapsed(),
                );
                for key in &commit.keys {
                    state.durability.reserved.remove(key);
                }
                if let Err(error) = super::publication::apply_committed_action(state, commit.action)
                {
                    state.durability.failed = true;
                    state.durability.pending = waiting;
                    state.durability.pending.extend(remaining);
                    return Err(io::Error::other(format!(
                        "WAL committed but in-memory apply failed; restarting is required: {error}"
                    )));
                }
            }
            Ok(Err(error)) => {
                state.metrics.record_latency(
                    LatencyEvent::DurableWalReceipt,
                    commit.submitted_at.elapsed(),
                );
                for key in &commit.keys {
                    state.durability.reserved.remove(key);
                }
                state.durability.failed = true;
                state.durability.pending = waiting;
                state.durability.pending.extend(remaining);
                return Err(io::Error::other(format!(
                    "durable WAL write failed: {error}"
                )));
            }
            Err(TryRecvError::Empty) => waiting.push(commit),
            Err(TryRecvError::Disconnected) => {
                state.durability.failed = true;
                for key in &commit.keys {
                    state.durability.reserved.remove(key);
                }
                state.durability.pending = waiting;
                state.durability.pending.extend(remaining);
                return Err(io::Error::other("durable WAL worker disconnected"));
            }
        }
    }
    state.durability.pending = waiting;
    Ok(())
}

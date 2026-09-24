//! Background batching, fsync, and receipt delivery loop.

use super::{Request, WriterCommand};
use crate::server::journal::Journal;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

pub(super) fn writer_loop(
    mut journal: Journal,
    requests: Receiver<WriterCommand>,
    batch_delay: Duration,
    usage: Arc<AtomicU64>,
    projected_usage: Arc<AtomicU64>,
    sequence: Arc<AtomicU64>,
    rotation_pending: Arc<AtomicBool>,
) -> io::Result<()> {
    let mut pending_command = None;
    loop {
        let command = match pending_command.take().or_else(|| requests.recv().ok()) {
            Some(command) => command,
            None => return Ok(()),
        };
        let mut batch = match command {
            WriterCommand::Append(request) => vec![request],
            WriterCommand::AppendBatch(requests) => requests,
            WriterCommand::Rotate {
                expected_sequence,
                compaction,
                acknowledge,
            } => {
                let result = match compaction {
                    Some(compaction) => {
                        journal.rotate_with_drop_compaction(expected_sequence, compaction)
                    }
                    None => journal.rotate(expected_sequence),
                };
                usage.store(journal.bytes(), Ordering::Release);
                if result.is_ok() {
                    projected_usage.store(journal.bytes(), Ordering::Release);
                }
                // A bad expected sequence is a recoverable caller error. A
                // failed manifest switch poisons the journal; keep admissions
                // closed in that case until restart resolves the generation.
                if journal.poisoned.is_none() {
                    rotation_pending.store(false, Ordering::Release);
                }
                sequence.store(journal.sequence(), Ordering::Release);
                let _ = acknowledge.send(result);
                continue;
            }
        };
        let deadline = Instant::now() + batch_delay.min(Duration::from_millis(250));
        while batch.len() < super::super::MAX_BATCH_RECORDS {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match requests.recv_timeout(deadline.saturating_duration_since(now)) {
                Ok(WriterCommand::Append(request)) => batch.push(request),
                Ok(WriterCommand::AppendBatch(mut next))
                    if batch.len() + next.len() <= super::super::MAX_BATCH_RECORDS =>
                {
                    batch.append(&mut next);
                }
                Ok(next @ WriterCommand::AppendBatch(_))
                | Ok(next @ WriterCommand::Rotate { .. }) => {
                    pending_command = Some(next);
                    break;
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        // Catch requests already queued if the delay is zero or elapsed during
        // a wakeup. The bound prevents an unending stream starving acknowledgments.
        while batch.len() < super::super::MAX_BATCH_RECORDS {
            match requests.try_recv() {
                Ok(WriterCommand::Append(request)) => batch.push(request),
                Ok(WriterCommand::AppendBatch(mut next))
                    if batch.len() + next.len() <= super::super::MAX_BATCH_RECORDS =>
                {
                    batch.append(&mut next);
                }
                Ok(next @ WriterCommand::AppendBatch(_))
                | Ok(next @ WriterCommand::Rotate { .. }) => {
                    pending_command = Some(next);
                    break;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        let reserved_bytes = batch
            .iter()
            .map(|request: &Request| request.reserved_bytes)
            .sum::<u64>();
        let acknowledgments = batch
            .iter()
            .map(|request| request.acknowledge.clone())
            .collect::<Vec<_>>();
        let old_bytes = journal.bytes();
        let results = journal.append_batch(batch);
        let appended_bytes = journal.bytes().saturating_sub(old_bytes);
        usage.store(journal.bytes(), Ordering::Release);
        debug_assert!(reserved_bytes >= appended_bytes);
        projected_usage.fetch_sub(reserved_bytes - appended_bytes, Ordering::AcqRel);
        sequence.store(journal.sequence(), Ordering::Release);
        for (acknowledge, result) in acknowledgments.into_iter().zip(results) {
            let _ = acknowledge.send(result);
        }
    }
}

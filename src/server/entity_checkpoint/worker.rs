//! Worker-owned mirror replay and atomic BGEN publication.

use super::{CheckpointReceipt, CheckpointWork, Command, Event, Shared};
#[cfg(test)]
use crate::server::entities::EntityMotionSnapshot;
use crate::server::entities::{EntityCheckpointStore, EntityStore, write_checkpoint};
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;

pub(super) fn run(
    receiver: Receiver<Command>,
    shared: Arc<Shared>,
    mut mirror: EntityStore,
    checkpoint_store: EntityCheckpointStore,
    mut work: CheckpointWork,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_loop(receiver, &shared, &mut mirror, &checkpoint_store, &mut work)
    }));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(reason)) => shared.fail(reason),
        Err(_) => shared.fail("entity checkpoint worker panicked"),
    }
}

fn run_loop(
    receiver: Receiver<Command>,
    shared: &Shared,
    mirror: &mut EntityStore,
    checkpoint_store: &EntityCheckpointStore,
    work: &mut CheckpointWork,
) -> Result<(), String> {
    let mut applied_sequence = 0u64;
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Event { sequence, event } => {
                let result = (|| {
                    if sequence
                        != applied_sequence
                            .checked_add(1)
                            .ok_or("event sequence exhausted")?
                    {
                        return Err("entity mirror event sequence gap".to_owned());
                    }
                    match event {
                        Event::Durable(batch) => {
                            let before = mirror.durable_sequence();
                            mirror.apply_committed_mirror(batch).map_err(|error| {
                                format!("entity mirror durable replay: {error}")
                            })?;
                            if mirror.durable_sequence() != before + 1 {
                                return Err("entity mirror durable sequence did not advance".into());
                            }
                        }
                        #[cfg(test)]
                        Event::Motion(snapshot) => apply_motion(mirror, snapshot)?,
                    }
                    Ok(())
                })();
                shared.outstanding.fetch_sub(1, Ordering::AcqRel);
                result?;
                applied_sequence = sequence;
                shared.applied_sequence.store(sequence, Ordering::Release);
            }
            Command::Checkpoint {
                required_sequence,
                reply,
            } => {
                let result = (|| -> io::Result<CheckpointReceipt> {
                    if applied_sequence != required_sequence {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "entity checkpoint fence missed an event",
                        ));
                    }
                    checkpoint_store.write_stream(|file| {
                        write_checkpoint(mirror, file, work.entries, |count| {
                            // This dedicated worker has no competing accepted
                            // work while fenced. Yield between bounded turns;
                            // never admit events into the captured generation.
                            work.after_turn(count)
                        })
                    })?;
                    Ok(CheckpointReceipt {
                        event_sequence: applied_sequence,
                        durable_sequence: mirror.durable_sequence(),
                        registry_revision: mirror.revision(),
                    })
                })();
                shared.outstanding.fetch_sub(1, Ordering::AcqRel);
                match result {
                    Ok(receipt) => {
                        shared
                            .checkpoint_sequence
                            .store(receipt.event_sequence, Ordering::Release);
                        reply
                            .send(Ok(receipt))
                            .map_err(|_| "entity checkpoint receipt abandoned".to_owned())?;
                    }
                    Err(error) => {
                        let message = format!("entity checkpoint write: {error}");
                        let _ = reply.send(Err(error));
                        return Err(message);
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn apply_motion(mirror: &mut EntityStore, snapshot: EntityMotionSnapshot) -> Result<(), String> {
    let previous = snapshot
        .revision
        .checked_sub(1)
        .ok_or_else(|| "entity mirror motion revision underflow".to_owned())?;
    let current = mirror
        .mobile_motion_snapshot(snapshot.id)
        .ok_or_else(|| "entity mirror motion references missing mobile entity".to_owned())?;
    if current.revision != previous {
        return Err("entity mirror motion is stale or skipped a revision".into());
    }
    mirror
        .update_mobile_motion(snapshot.id, previous, snapshot.position)
        .map_err(|error| format!("entity mirror motion replay: {error}"))?;
    let applied = mirror
        .mobile_motion_snapshot(snapshot.id)
        .ok_or_else(|| "entity mirror motion owner disappeared".to_owned())?;
    if applied.revision != snapshot.revision
        || applied.position.map(f32::to_bits) != snapshot.position.map(f32::to_bits)
    {
        return Err("entity mirror motion result differs from committed snapshot".into());
    }
    Ok(())
}

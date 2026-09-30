//! Server-owned clock with periodic checkpoints on a dedicated I/O worker.
use crate::daylight::{CYCLE_MS, INITIAL_MS};
use std::{
    io,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::server::journal::{Change, StateKey};
mod persistence;
pub(super) use persistence::{prepare_recovery, replay};

pub(super) const DOMAIN: &str = "bloxgloom:world_time";
pub(super) fn state_key() -> StateKey {
    StateKey::new(DOMAIN, Vec::new())
}

#[derive(Clone, Debug)]
pub(super) struct ReadStamp {
    revision: Arc<AtomicU64>,
    captured: u64,
}
impl ReadStamp {
    pub(super) fn is_current(&self) -> bool {
        self.revision.load(Ordering::Acquire) == self.captured
    }
}
pub(super) struct Capture {
    pub stamp: ReadStamp,
    pub time: bloxgloom_host_api::gameplay::WorldTime,
}

pub(super) struct Clock {
    anchor: Vec<u8>,
    revision: Arc<AtomicU64>,
    initial: u64,
    started: Instant,
    last_publish: Instant,
    last_save: Instant,
    checkpoint_pending: bool,
    sender: Option<SyncSender<persistence::Snapshot>>,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl Clock {
    pub(super) fn open(root: &Path) -> io::Result<Self> {
        let path = root.join("world.time");
        let snapshot = persistence::read(root)?;
        let revision =
            persistence::decode_anchor(&snapshot.anchor)?.map_or(0, |(revision, _)| revision);
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("world-clock-save".into())
            .spawn(move || {
                while let Ok(time) = receiver.recv() {
                    persistence::save(&path, &time)?;
                }
                Ok(())
            })?;
        let started = Instant::now();
        Ok(Self {
            anchor: snapshot.anchor,
            revision: Arc::new(AtomicU64::new(revision)),
            initial: snapshot.elapsed_ms,
            started,
            last_publish: started,
            last_save: started,
            checkpoint_pending: false,
            sender: Some(sender),
            worker: Some(worker),
        })
    }

    pub(super) fn now(&self) -> u64 {
        (self.initial + (self.started.elapsed().as_millis() % u128::from(CYCLE_MS)) as u64)
            % CYCLE_MS
    }

    pub(super) fn capture(&self) -> Capture {
        Capture {
            stamp: ReadStamp {
                revision: self.revision.clone(),
                captured: self.revision.load(Ordering::Acquire),
            },
            time: bloxgloom_host_api::gameplay::WorldTime {
                elapsed_ms: self.now(),
                cycle_ms: CYCLE_MS,
            },
        }
    }

    pub(super) fn prepare(&self, time: u64) -> io::Result<Change> {
        if time >= CYCLE_MS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid world time",
            ));
        }
        let revision = self
            .revision
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or_else(|| io::Error::other("world time revision exhausted"))?;
        let mut after = revision.to_le_bytes().to_vec();
        after.extend(time.to_le_bytes());
        Ok(Change::new(state_key(), self.anchor.clone(), after))
    }

    pub(super) fn validate(&self, change: &Change) -> io::Result<()> {
        if change.key != state_key() || change.before != self.anchor {
            return Err(io::Error::other(
                "committed world time precondition changed",
            ));
        }
        let (revision, _) = persistence::decode_anchor(&change.after)?
            .ok_or_else(|| io::Error::other("missing committed world time"))?;
        if self.revision.load(Ordering::Acquire).checked_add(1) != Some(revision) {
            return Err(io::Error::other("invalid world time revision"));
        }
        Ok(())
    }

    pub(super) fn apply(&mut self, change: Change) -> io::Result<()> {
        self.validate(&change)?;
        let (revision, time) = persistence::decode_anchor(&change.after)?.unwrap();
        self.anchor = change.after;
        self.revision.store(revision, Ordering::Release);
        self.set(time)
    }

    fn snapshot(&self) -> persistence::Snapshot {
        persistence::Snapshot {
            anchor: self.anchor.clone(),
            elapsed_ms: self.now(),
        }
    }

    pub(super) fn poll(&mut self) -> Option<u64> {
        if self.checkpoint_pending || self.last_save.elapsed() >= Duration::from_secs(5) {
            self.checkpoint();
        }
        if self.last_publish.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.last_publish = Instant::now();
        Some(self.now())
    }

    fn checkpoint(&mut self) {
        if self
            .sender
            .as_ref()
            .is_some_and(|sender| sender.try_send(self.snapshot()).is_ok())
        {
            self.last_save = Instant::now();
            self.checkpoint_pending = false;
        }
    }

    fn set(&mut self, time: u64) -> io::Result<()> {
        if time >= CYCLE_MS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid world time",
            ));
        }
        self.initial = time;
        self.started = Instant::now();
        self.last_publish = Instant::now();
        self.checkpoint_pending = true;
        self.checkpoint();
        Ok(())
    }

    pub(super) fn finish(&mut self) -> io::Result<()> {
        if let Some(sender) = self.sender.take() {
            // Shutdown may wait for the worker; normal ticks never wait on I/O.
            let _ = sender.send(self.snapshot());
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("world clock worker panicked"))??;
        }
        Ok(())
    }
}

pub(super) fn publish(state: &super::State) {
    let elapsed_ms = state.world_time.now();
    for client in state.clients.values() {
        // Advisory updates are repaired by the next periodic sample.
        let _ = client
            .sender
            .try_send(crate::protocol::ServerMessage::WorldTime { elapsed_ms });
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            tracing::error!(%error, "world clock save failed");
        }
    }
}

#[cfg(test)]
#[path = "world_time/tests.rs"]
mod tests;

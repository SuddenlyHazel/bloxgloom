//! Persistent weather clock. Only admin changes enter the WAL; natural evolution
//! is derived from a saved monotonic clock and seed, independent of tick batching.
use super::journal::{Change, StateKey};
use crate::weather::{WeatherKind, WeatherSnapshot, mix};
use std::{
    io,
    path::Path,
    sync::mpsc::{self, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
mod persistence;
pub(super) use persistence::{prepare_recovery, replay};
pub(super) const DOMAIN: &str = "bloxgloom:weather";
pub(super) fn state_key() -> StateKey {
    StateKey::new(DOMAIN, Vec::new())
}
pub(super) struct Clock {
    anchor: Vec<u8>,
    initial: WeatherSnapshot,
    started: Instant,
    last_publish: Instant,
    last_save: Instant,
    sender: Option<SyncSender<persistence::Checkpoint>>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
impl Clock {
    pub(super) fn open(root: &Path, seed: u64) -> io::Result<Self> {
        let c = persistence::read(root, seed)?;
        let path = root.join("world.weather");
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("weather-save".into())
            .spawn(move || {
                while let Ok(c) = receiver.recv() {
                    persistence::save(&path, &c)?
                }
                Ok(())
            })?;
        let now = Instant::now();
        Ok(Self {
            anchor: c.anchor,
            initial: c.snapshot,
            started: now,
            last_publish: now,
            last_save: now,
            sender: Some(sender),
            worker: Some(worker),
        })
    }
    pub(super) fn snapshot(&self) -> WeatherSnapshot {
        let s = self.initial;
        let elapsed = s
            .elapsed_ms
            .saturating_add(self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        advance(s, elapsed)
    }

    pub(super) fn prepare(&self, kind: u8, transition_ms: u32) -> io::Result<Change> {
        let mut s = self.snapshot();
        s.from = s.sample_at(s.elapsed_ms);
        s.to = WeatherKind::from_u8(kind).ok_or_else(persistence::invalid)?;
        if transition_ms > 60_000 {
            return Err(persistence::invalid());
        }
        s.transition_start_ms = s.elapsed_ms;
        s.transition_duration_ms = transition_ms;
        s.next_change_ms = s.elapsed_ms.saturating_add(300_000);
        s.revision = s.revision.checked_add(1).ok_or_else(persistence::invalid)?;
        Ok(Change::new(
            state_key(),
            self.anchor.clone(),
            crate::weather::codec::encode(s),
        ))
    }
    pub(super) fn validate(&self, c: &Change) -> io::Result<()> {
        let s = crate::weather::codec::decode(&c.after).ok_or_else(persistence::invalid)?;
        if c.key != state_key()
            || c.before != self.anchor
            || self.initial.revision.checked_add(1) != Some(s.revision)
        {
            return Err(persistence::invalid());
        }
        Ok(())
    }
    pub(super) fn apply(&mut self, c: Change) -> io::Result<()> {
        self.validate(&c)?;
        self.initial = crate::weather::codec::decode(&c.after).unwrap();
        self.anchor = c.after;
        self.started = Instant::now();
        self.last_save = self.started - Duration::from_secs(5);
        Ok(())
    }
    fn checkpoint(&self) -> persistence::Checkpoint {
        persistence::Checkpoint {
            anchor: self.anchor.clone(),
            snapshot: self.snapshot(),
        }
    }
    pub(super) fn poll(&mut self) -> Option<WeatherSnapshot> {
        if self.last_save.elapsed() >= Duration::from_secs(5)
            && self
                .sender
                .as_ref()
                .is_some_and(|s| s.try_send(self.checkpoint()).is_ok())
        {
            self.last_save = Instant::now()
        }
        if self.last_publish.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.last_publish = Instant::now();
        let s = self.snapshot();
        self.initial = s;
        self.started = self.last_publish;
        Some(s)
    }
    pub(super) fn finish(&mut self) -> io::Result<()> {
        if let Some(s) = self.sender.take() {
            let _ = s.send(self.checkpoint());
        }
        if let Some(w) = self.worker.take() {
            w.join()
                .map_err(|_| io::Error::other("weather worker panicked"))??;
        }
        Ok(())
    }
}
impl Drop for Clock {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            tracing::error!(%error,"weather save failed")
        }
    }
}
pub(super) fn publish(state: &super::State) {
    let snapshot = state.weather.snapshot();
    for client in state.clients.values() {
        let _ = client
            .sender
            .try_send(crate::protocol::ServerMessage::Weather { snapshot });
    }
}
#[cfg(test)]
mod tests;

fn advance(mut s: WeatherSnapshot, elapsed: u64) -> WeatherSnapshot {
    while elapsed >= s.next_change_ms {
        let at = s.next_change_ms;
        let hash = mix(s.seed ^ at);
        s.from = s.sample_at(at);
        s.to = match hash % 10 {
            0..=4 => WeatherKind::Clear,
            5..=7 => WeatherKind::Rain,
            _ => WeatherKind::Storm,
        };
        s.transition_start_ms = at;
        s.transition_duration_ms = 30_000;
        s.next_change_ms = at.saturating_add(180_000 + hash % 180_000);
        if s.next_change_ms == at {
            break;
        }
    }
    s.elapsed_ms = elapsed;
    s
}

//! Bounded, coalescing mesh mailbox. Three immediate jobs buy one background
//! turn when both lanes have work. Both workers share the same fairness cursor.
use super::workers::MesherJob;
use crate::world::ChunkKey;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, mpsc::TrySendError};

struct State {
    jobs: HashMap<ChunkKey, (bool, MesherJob)>,
    immediate: VecDeque<ChunkKey>,
    background: VecDeque<ChunkKey>,
    burst: u8,
    senders: u8,
}
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
}
pub(super) struct Sender {
    shared: Arc<Shared>,
    immediate: bool,
}
#[derive(Clone)]
pub(super) struct Receiver {
    shared: Arc<Shared>,
}

pub(super) fn channel() -> (Sender, Sender, Receiver) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            jobs: HashMap::new(),
            immediate: VecDeque::new(),
            background: VecDeque::new(),
            burst: 0,
            senders: 2,
        }),
        ready: Condvar::new(),
    });
    (
        Sender {
            shared: shared.clone(),
            immediate: false,
        },
        Sender {
            shared: shared.clone(),
            immediate: true,
        },
        Receiver { shared },
    )
}
impl Sender {
    pub(super) fn invalidate(&self, key: ChunkKey) {
        let mut state = self.shared.state.lock().unwrap();
        state.jobs.remove(&key);
        state.immediate.retain(|k| *k != key);
        state.background.retain(|k| *k != key);
    }
    pub(super) fn try_send(&self, job: MesherJob) -> Result<(), TrySendError<MesherJob>> {
        let mut state = self.shared.state.lock().unwrap();
        let key = job.chunk.key;
        let previous = state
            .jobs
            .get(&key)
            .map(|(urgent, job)| (*urgent, job.revision));
        if previous.is_some_and(|(_, revision)| revision > job.revision) {
            return Ok(());
        }
        let urgent = self.immediate || previous.is_some_and(|(urgent, _)| urgent);
        if (urgent && !previous.is_some_and(|(urgent, _)| urgent) && state.immediate.len() == 16)
            || (!urgent && previous.is_none() && state.background.len() == 64)
        {
            // Separate caps reserve space for both lanes. The window retains
            // failed submissions and retries; authoritative work is never lost.
            return Err(TrySendError::Full(job));
        }
        if previous.is_none() || previous.is_some_and(|(old, _)| !old && urgent) {
            state.background.retain(|k| *k != key);
            if urgent {
                state.immediate.push_back(key);
            } else {
                state.background.push_back(key);
            }
        }
        state.jobs.insert(key, (urgent, job));
        self.shared.ready.notify_one();
        Ok(())
    }
}
impl Drop for Sender {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().senders -= 1;
        self.shared.ready.notify_all();
    }
}
impl Receiver {
    pub(super) fn recv(&self) -> Option<MesherJob> {
        let mut state = self.shared.state.lock().unwrap();
        loop {
            let immediate =
                !state.immediate.is_empty() && (state.burst < 3 || state.background.is_empty());
            let key = if immediate {
                state.burst = (state.burst + 1).min(3);
                state.immediate.pop_front()
            } else {
                state.burst = 0;
                state.background.pop_front()
            };
            if let Some(key) = key {
                return state.jobs.remove(&key).map(|(_, job)| job);
            }
            if state.senders == 0 {
                return None;
            }
            state = self.shared.ready.wait(state).unwrap();
        }
    }
}

#[cfg(test)]
#[path = "mesh_queue_tests.rs"]
mod tests;

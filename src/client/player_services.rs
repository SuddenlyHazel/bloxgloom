//! Session-scoped callbacks, snapshot coalescing and out-of-band disconnect.
//! Lua and imports execute on this worker, never the window or network reader.
use super::startup::{self, State, players::Event};
use crate::{protocol::PlayerState, server::client_bundle::ClientBundle};
use std::sync::{Arc, Condvar, Mutex, mpsc};

#[derive(Clone)]
struct Snapshot {
    profile: u128,
    session: u64,
    states: Vec<PlayerState>,
}
#[derive(Default)]
struct Mailbox {
    latest: Option<Snapshot>,
    close: Option<String>,
}
type Shared = Arc<(Mutex<Mailbox>, Condvar)>;

pub(super) struct Lane {
    shared: Shared,
    pub(super) replies: mpsc::Receiver<State>,
    #[cfg(test)]
    events: mpsc::Receiver<(&'static str, usize)>,
}
impl Lane {
    pub(super) fn spawn(
        bundle: Arc<ClientBundle>,
        startup: &State,
        profile: u128,
        session: u64,
        states: Vec<PlayerState>,
    ) -> std::io::Result<Option<Self>> {
        if startup.player_handlers.is_empty() {
            return Ok(None);
        }
        let shared = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let mailbox = Arc::clone(&shared);
        let (sender, replies) = mpsc::sync_channel(1);
        #[cfg(test)]
        let (events_tx, events) = mpsc::channel();
        let handlers = startup.player_handlers.clone();
        let mut parameters = startup.parameters.clone();
        parameters.take_updates();
        let mut snapshot = Snapshot {
            profile,
            session,
            states,
        };
        std::thread::Builder::new()
            .name("client-player-services".into())
            .spawn(move || {
                let mut runner = Runner {
                    bundle,
                    handlers,
                    parameters,
                    sender,
                };
                let failures = runner.deliver(&snapshot, "SessionReady", "", true);
                #[cfg(test)]
                let _ = events_tx.send(("SessionReady", failures));
                #[cfg(not(test))]
                let _ = failures;
                loop {
                    let (lock, wake) = &*mailbox;
                    let mut pending = lock.lock().unwrap();
                    while pending.latest.is_none() && pending.close.is_none() {
                        pending = wake.wait(pending).unwrap();
                    }
                    let next = pending.latest.take();
                    let close = pending.close.take();
                    drop(pending);
                    if let Some(next) = next {
                        snapshot = next;
                    }
                    if let Some(reason) = close {
                        let failures =
                            runner.deliver(&snapshot, "SessionDisconnected", &reason, false);
                        #[cfg(test)]
                        let _ = events_tx.send(("SessionDisconnected", failures));
                        #[cfg(not(test))]
                        let _ = failures;
                        break;
                    }
                    let failures = runner.deliver(&snapshot, "PlayerStateChanged", "", true);
                    #[cfg(test)]
                    let _ = events_tx.send(("PlayerStateChanged", failures));
                    #[cfg(not(test))]
                    let _ = failures;
                }
            })?;
        Ok(Some(Self {
            shared,
            replies,
            #[cfg(test)]
            events,
        }))
    }
    pub(super) fn update(&self, profile: u128, session: u64, states: Vec<PlayerState>) {
        let (lock, wake) = &*self.shared;
        let mut pending = lock.lock().unwrap();
        if pending.close.is_none() {
            pending.latest = Some(Snapshot {
                profile,
                session,
                states,
            });
            wake.notify_one();
        }
    }
    pub(super) fn close(&self, reason: &str) {
        let (lock, wake) = &*self.shared;
        let mut pending = lock.lock().unwrap();
        if pending.close.is_none() {
            pending.close = Some(reason.chars().take(512).collect());
            wake.notify_one();
        }
    }
}
impl Drop for Lane {
    fn drop(&mut self) {
        self.close("session retired");
    }
}

struct Runner {
    bundle: Arc<ClientBundle>,
    handlers: std::collections::BTreeMap<String, String>,
    parameters: crate::render::parameters::State,
    sender: mpsc::SyncSender<State>,
}
impl Runner {
    fn deliver(
        &mut self,
        snapshot: &Snapshot,
        kind: &'static str,
        reason: &str,
        publish: bool,
    ) -> usize {
        let mut failures = 0;
        for module in self.handlers.values() {
            let mut output = State {
                parameters: self.parameters.clone(),
                ..Default::default()
            };
            let event = Event {
                kind,
                profile: snapshot.profile,
                session: snapshot.session,
                states: &snapshot.states,
                reason,
            };
            match startup::execute_event(
                Arc::clone(&self.bundle),
                module,
                &mut output,
                Some(&event),
            ) {
                Ok(()) => {
                    self.parameters = output.parameters.clone();
                    self.parameters.take_updates();
                    // Blocking this worker's single reply slot never blocks a client
                    // thread. Retirement drops the receiver and wakes it immediately;
                    // it still runs the final disconnect hooks on its own thread.
                    if publish {
                        let _ = self.sender.send(output);
                    }
                }
                Err(error) => {
                    failures += 1;
                    tracing::warn!(%error, event=kind, "client player callback failed");
                }
            }
        }
        failures
    }
}

#[cfg(test)]
mod tests;

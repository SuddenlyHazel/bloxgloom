//! Admission decisions and bounded lifecycle jobs. Publication follows WAL sync.
use super::preparation::prepare;
use super::{State, capture};
use crate::server::{durable::StageError, registry::SystemId, simulation::TickId};
use bloxgloom_host_api::players::{Event, EventKind, Registration};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io,
    sync::Arc,
};

pub(super) const QUEUE_LIMIT: usize = 8192;
pub(super) type SessionKey = (String, u128, u64);
pub(super) struct Job {
    registration: Arc<Registration>,
    event: Event,
    final_session: Option<Vec<u8>>,
}
pub(super) type JobKey = (String, u128, u64, &'static str);
#[derive(Default)]
pub(super) struct Session {
    pub(super) data: Vec<u8>,
    pub(super) deadline: Option<u64>,
}
#[derive(Default)]
pub(in crate::server) struct Runtime {
    pub(super) tick: u64,
    pub(super) admitted: BTreeSet<(u128, u64)>,
    pub(super) sessions: BTreeMap<SessionKey, Session>,
    pub(super) queue: VecDeque<Job>,
    pub(super) active: BTreeSet<JobKey>,
    pub(super) failed: BTreeSet<JobKey>,
}
impl Runtime {
    pub(in crate::server) fn is_admitted(&self, profile: u128, epoch: u64) -> bool {
        self.admitted.contains(&(profile, epoch))
    }
    #[cfg(test)]
    pub(in crate::server) fn session_count(&self) -> usize {
        self.sessions.len()
    }
}
#[derive(Clone)]
pub(in crate::server) enum Published {
    Lifecycle {
        key: JobKey,
        session: Option<(SessionKey, Vec<u8>, Option<u64>)>,
        operations: Vec<bloxgloom_host_api::gameplay::PlayerOperation>,
    },
    Operations(Vec<bloxgloom_host_api::gameplay::PlayerOperation>),
}
impl Published {
    pub(in crate::server) fn operations(
        ops: Vec<bloxgloom_host_api::gameplay::PlayerOperation>,
    ) -> Option<Self> {
        (!ops.is_empty()).then_some(Self::Operations(ops))
    }
}
pub(super) fn key(reg: &Registration, event: &Event) -> JobKey {
    (
        reg.key.clone(),
        event.profile,
        event.transition,
        event.kind.name(),
    )
}
fn enqueue(
    runtime: &mut Runtime,
    reg: Arc<Registration>,
    event: Event,
    final_session: Option<Vec<u8>>,
) {
    let key = key(&reg, &event);
    if runtime.queue.len() < QUEUE_LIMIT
        && !runtime.failed.contains(&key)
        && runtime.active.insert(key)
    {
        runtime.queue.push_back(Job {
            registration: reg,
            event,
            final_session,
        });
    }
}
pub(in crate::server) fn joined(state: &mut State, id: u64) {
    let Some(player) = capture(state).into_iter().find(|p| {
        state
            .clients
            .get(&id)
            .is_some_and(|c| c.profile == p.profile)
    }) else {
        return;
    };
    state
        .player_runtime
        .admitted
        .insert((player.profile, player.session));
    state.roster_revision = state.roster_revision.wrapping_add(1);
    let regs = state
        .world
        .catalog()
        .player_lifecycles()
        .cloned()
        .collect::<Vec<_>>();
    for kind in [EventKind::Joined, EventKind::Spawned] {
        for reg in &regs {
            enqueue(
                &mut state.player_runtime,
                Arc::clone(reg),
                Event {
                    kind,
                    profile: player.profile,
                    transition: player.session,
                    player: Some(player.clone()),
                },
                None,
            );
        }
    }
}
pub(in crate::server) fn leaving(state: &mut State, id: u64) {
    let Some(client) = state.clients.get(&id) else {
        return;
    };
    let profile = client.profile;
    let epoch = client.action_epoch;
    if !state.player_runtime.admitted.remove(&(profile, epoch)) {
        return;
    }
    state.roster_revision = state.roster_revision.wrapping_add(1);
    let player = capture(state).into_iter().find(|p| p.profile == profile);
    let regs = state
        .world
        .catalog()
        .player_lifecycles()
        .cloned()
        .collect::<Vec<_>>();
    for kind in [EventKind::Leaving, EventKind::Left] {
        for reg in &regs {
            let final_session = state
                .player_runtime
                .sessions
                .get(&(reg.key.clone(), profile, epoch))
                .map(|s| s.data.clone());
            enqueue(
                &mut state.player_runtime,
                Arc::clone(reg),
                Event {
                    kind,
                    profile,
                    transition: epoch,
                    player: player.clone(),
                },
                final_session,
            );
        }
    }
    state
        .player_runtime
        .sessions
        .retain(|(_, p, e), _| *p != profile || *e != epoch);
    state
        .player_runtime
        .failed
        .retain(|(_, p, _, kind)| *p != profile || *kind == EventKind::ProfileTick.name());
}
pub(in crate::server) fn drive(state: &mut State, tick: TickId) -> io::Result<()> {
    state.player_runtime.tick = tick.get();
    let regs = state
        .world
        .catalog()
        .player_lifecycles()
        .cloned()
        .collect::<Vec<_>>();
    let players = capture(state);
    for reg in &regs {
        let system =
            SystemId::new(&reg.key).map_err(|_| io::Error::other("invalid player service"))?;
        for (due, profile) in state.system_runtime.due_profiles(&system, tick.get(), 4) {
            enqueue(
                &mut state.player_runtime,
                Arc::clone(reg),
                Event {
                    kind: EventKind::ProfileTick,
                    profile,
                    transition: due,
                    player: players.iter().find(|p| p.profile == profile).cloned(),
                },
                None,
            );
        }
    }
    let due_sessions = state
        .player_runtime
        .sessions
        .iter()
        .filter_map(|((key, profile, epoch), session)| {
            session
                .deadline
                .filter(|due| *due <= tick.get())
                .map(|due| (key.clone(), *profile, *epoch, due))
        })
        .take(4)
        .collect::<Vec<_>>();
    for (key, profile, epoch, due) in due_sessions {
        if let Some(player) = players
            .iter()
            .find(|p| p.profile == profile && p.session == epoch)
            && let Some(reg) = regs.iter().find(|reg| reg.key == key)
        {
            enqueue(
                &mut state.player_runtime,
                Arc::clone(reg),
                Event {
                    kind: EventKind::SessionTick,
                    profile,
                    transition: due,
                    player: Some(player.clone()),
                },
                None,
            );
        }
    }
    let attempts = state.player_runtime.queue.len().min(4);
    for _ in 0..attempts {
        let Some(Job {
            registration: reg,
            event,
            final_session,
        }) = state.player_runtime.queue.pop_front()
        else {
            break;
        };
        let job_key = key(&reg, &event);
        if event.kind == EventKind::SessionTick
            && event.player.as_ref().is_none_or(|p| {
                !state
                    .player_runtime
                    .admitted
                    .contains(&(p.profile, p.session))
            })
        {
            state.player_runtime.active.remove(&job_key);
            continue;
        }
        match prepare(state, &reg, &event, tick, final_session.as_deref()) {
            Ok(action) => match state.durability.try_stage(tick, &action, None) {
                Ok(true) => {}
                Ok(false) | Err(StageError::Conflict | StageError::Full) => {
                    state.player_runtime.queue.push_front(Job {
                        registration: reg,
                        event,
                        final_session,
                    });
                    break;
                }
                Err(error) => {
                    return Err(io::Error::other(format!(
                        "player lifecycle stage failed: {error:?}"
                    )));
                }
            },
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                state.player_runtime.queue.push_back(Job {
                    registration: reg,
                    event,
                    final_session,
                });
            }
            Err(error) => {
                tracing::warn!(%error, service=%reg.key, profile=%event.profile, event=event.kind.name(),"player lifecycle rejected without publishing effects");
                state.player_runtime.active.remove(&job_key);
                if matches!(event.kind, EventKind::ProfileTick | EventKind::SessionTick) {
                    state.player_runtime.failed.insert(job_key);
                }
            }
        }
    }
    Ok(())
}
pub(in crate::server) fn committed(state: &mut State, published: Published) -> io::Result<()> {
    let Published::Lifecycle {
        key,
        session,
        operations,
    } = published
    else {
        if let Published::Operations(operations) = published {
            return super::operations::apply(state, operations);
        }
        return Ok(());
    };
    state.player_state_revision = state
        .player_state_revision
        .checked_add(1)
        .ok_or_else(|| io::Error::other("player state revision exhausted"))?;
    for client in state.clients.values_mut().filter(|c| c.profile == key.1) {
        client.last_player_state_revision = 0;
    }
    state.player_runtime.active.remove(&key);
    state
        .player_runtime
        .failed
        .retain(|(reg, profile, _, _)| reg != &key.0 || *profile != key.1);
    if let Some((session_key, data, deadline)) = session {
        if state
            .player_runtime
            .admitted
            .contains(&(session_key.1, session_key.2))
        {
            state
                .player_runtime
                .sessions
                .insert(session_key, Session { data, deadline });
        } else if key.3 == EventKind::Left.name() {
            state.player_runtime.sessions.remove(&session_key);
        }
    }
    super::operations::apply(state, operations)
}

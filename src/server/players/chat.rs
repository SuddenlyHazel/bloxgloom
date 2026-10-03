//! Authenticated chat admission, bounded moderation lane and session-fenced delivery.
use super::{State, capture};
use bloxgloom_host_api::chat::{Message, Request, valid_text};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
mod worker;
#[derive(Default)]
pub(in crate::server) struct Runtime {
    sessions: BTreeMap<(u128, u64), Admission>,
    next_message: u64,
    lane: Option<worker::Lane>,
}
struct Admission {
    sequence: u64,
    window: Instant,
    count: u8,
}
/// Every request consumes its monotonic sequence, including denial and throttling.
/// One session may send at most four requests per two seconds.
pub(in crate::server) fn receive(state: &mut State, id: u64, sequence: u64, text: String) {
    let Some(client) = state.clients.get(&id) else {
        return;
    };
    let key = (client.profile, client.action_epoch);
    if !state.player_runtime.is_admitted(key.0, key.1) {
        return;
    }
    let now = Instant::now();
    let admission = state.chat_runtime.sessions.entry(key).or_insert(Admission {
        sequence: 0,
        window: now,
        count: 0,
    });
    if sequence == 0 || sequence <= admission.sequence {
        return;
    }
    admission.sequence = sequence;
    if now.duration_since(admission.window) >= Duration::from_secs(2) {
        admission.window = now;
        admission.count = 0;
    }
    admission.count = admission.count.saturating_add(1);
    if admission.count > 4 {
        reject(state, id, "Chat rate limit: four messages per two seconds.");
        return;
    }
    if !valid_text(&text) {
        reject(
            state,
            id,
            "Chat must contain 1–512 UTF-8 bytes without control characters.",
        );
        return;
    }
    let players = capture(state)
        .into_iter()
        .filter(|p| state.player_runtime.is_admitted(p.profile, p.session))
        .collect::<Vec<_>>();
    let Some(sender) = players
        .iter()
        .find(|p| p.profile == key.0 && p.session == key.1)
        .cloned()
    else {
        return;
    };
    let request = Request {
        sender,
        text,
        players,
    };
    let hooks = state.world.catalog().chat_hooks().cloned().collect();
    if state.chat_runtime.lane.is_none() {
        match worker::Lane::spawn() {
            Ok(lane) => state.chat_runtime.lane = Some(lane),
            Err(_) => {
                reject(state, id, "Chat moderation worker unavailable.");
                return;
            }
        }
    }
    if !state
        .chat_runtime
        .lane
        .as_ref()
        .unwrap()
        .enqueue(worker::Job { request, hooks })
    {
        reject(state, id, "Chat moderation is busy; please retry.");
    }
}
/// Drain bounded results at tick publication. Sender and recipients must still be
/// the exact admitted sessions captured before background moderation started.
pub(in crate::server) fn publish(state: &mut State) {
    for _ in 0..16 {
        let Some(result) = state
            .chat_runtime
            .lane
            .as_ref()
            .and_then(worker::Lane::next)
        else {
            break;
        };
        let key = (result.sender.profile, result.sender.session);
        let Some(id) = state
            .clients
            .iter()
            .find(|(_, c)| {
                (c.profile, c.action_epoch) == key && state.player_runtime.is_admitted(key.0, key.1)
            })
            .map(|(id, _)| *id)
        else {
            continue;
        };
        match result.output {
            Err(reason) => reject(state, id, &reason),
            Ok((text, recipients)) => {
                let Some(message_id) = state.chat_runtime.next_message.checked_add(1) else {
                    continue;
                };
                state.chat_runtime.next_message = message_id;
                let message = Message {
                    id: message_id,
                    profile: key.0,
                    session: key.1,
                    name: result.sender.name,
                    text,
                };
                for client in state.clients.values_mut() {
                    if state
                        .player_runtime
                        .is_admitted(client.profile, client.action_epoch)
                        && recipients.contains(&(client.profile, client.action_epoch))
                    {
                        let _ = client
                            .sender
                            .try_send(crate::protocol::ServerMessage::Chat {
                                message: message.clone(),
                            });
                    }
                }
            }
        }
    }
}
fn reject(state: &mut State, id: u64, reason: &str) {
    if let Some(client) = state.clients.get_mut(&id) {
        let _ = client
            .sender
            .try_send(crate::protocol::ServerMessage::ChatRejected {
                text: reason.into(),
            });
    }
}
pub(in crate::server) fn leaving(state: &mut State, id: u64) {
    if let Some(client) = state.clients.get(&id) {
        state
            .chat_runtime
            .sessions
            .remove(&(client.profile, client.action_epoch));
    }
}

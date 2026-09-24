//! Tick-thread admission and scheduling for durable gameplay requests.

use super::actions::plan_durable_request;
use super::checkpoint::{process_checkpoint_receipts, submit_dirty_checkpoints};
use super::receipt::poll_journal_receipts;
use super::rotation::progress_rotation;
use super::*;
use crate::server::{State, durable, handle_message};

enum PlannedAction {
    Commit(Box<CommitAction>),
    Reject(String),
    NoChange,
}

pub(in crate::server) fn handle_live_message(
    state: &mut State,
    id: u64,
    message: ClientMessage,
    _tick: TickId,
) -> io::Result<()> {
    let action_id = match &message {
        ClientMessage::Edit { action_id, .. }
        | ClientMessage::InventoryMove { action_id, .. }
        | ClientMessage::DropStack { action_id, .. } => Some(*action_id),
        _ => None,
    };
    if let Some(action_id) = action_id {
        if state.durability.queued.len() >= MAX_DEFERRED_DURABLE_ACTIONS {
            queue_action_result(state, id, action_id, false, "durable action queue full");
        } else {
            state.durability.queued.push_back(DurableRequest::Command {
                id,
                message,
                queued_at: Instant::now(),
            });
        }
        return Ok(());
    }
    handle_message(state, id, message)
}

pub(in crate::server) fn process_durable_actions(
    state: &mut State,
    tick: TickId,
    now: Instant,
) -> io::Result<()> {
    process_checkpoint_receipts(state, now);
    poll_journal_receipts(state)?;
    if state.durability.failed {
        return Err(io::Error::other("durable subsystem failed"));
    }
    if progress_rotation(state)? {
        submit_dirty_checkpoints(state);
        return fail_if_durability_failed(state);
    }

    let mut attempts = state
        .durability
        .queued
        .len()
        .min(MAX_PENDING_DURABLE_ACTIONS);
    let mut deferred = std::collections::VecDeque::new();
    let mut blocked_profiles = HashSet::new();
    while attempts > 0 {
        attempts -= 1;
        let Some(request) = state.durability.queued.pop_front() else {
            break;
        };
        let request_profile = durable_request_profile(state, &request);
        if let Some(profile) = request_profile
            && (blocked_profiles.contains(&profile) || state.durability.profile_pending(profile))
        {
            blocked_profiles.insert(profile);
            deferred.push_back(request);
            continue;
        }
        if matches!(
            &request,
            DurableRequest::Command { queued_at, .. }
                if queued_at.elapsed() > Duration::from_secs(10)
        ) {
            finish_rejected_request(
                state,
                &request,
                "target state unavailable; retry action".into(),
            );
            continue;
        }
        let plan = match plan_durable_request(state, &request) {
            Ok(Some(plan)) => PlannedAction::Commit(Box::new(plan)),
            Ok(None) => PlannedAction::NoChange,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if let Some(profile) = request_profile {
                    blocked_profiles.insert(profile);
                }
                deferred.push_back(request);
                continue;
            }
            Err(error) => PlannedAction::Reject(error.to_string()),
        };
        match plan {
            PlannedAction::NoChange => finish_empty_request(state, &request),
            PlannedAction::Reject(reason) => finish_rejected_request(state, &request, reason),
            PlannedAction::Commit(action) => {
                let action = *action;
                let projected_drop_snapshot_size =
                    if action.drops.changes.is_empty() && action.drops.allocator.is_none() {
                        None
                    } else {
                        Some(state.drops.projected_snapshot_size(&action.drops)?)
                    };
                match state
                    .durability
                    .try_stage(tick, &action, projected_drop_snapshot_size)
                {
                    Ok(true) => {
                        if let Some(profile) = action.profile {
                            blocked_profiles.insert(profile);
                        }
                    }
                    Ok(false) => finish_empty_request(state, &request),
                    Err(StageError::Conflict | StageError::Full) => {
                        if let Some(profile) = request_profile {
                            blocked_profiles.insert(profile);
                        }
                        deferred.push_back(request);
                    }
                    Err(StageError::Closed) => {
                        state.durability.failed = true;
                        return Err(io::Error::other("durability writer unavailable"));
                    }
                    Err(StageError::ReceiptLimit) => {
                        finish_rejected_request(
                            state,
                            &request,
                            "action receipt limit reached".into(),
                        );
                    }
                    Err(StageError::IdExhausted) => {
                        state.durability.failed = true;
                        return Err(io::Error::other("durable transaction IDs exhausted"));
                    }
                    Err(StageError::Invalid(error)) => {
                        state.durability.failed = true;
                        return Err(io::Error::other(format!(
                            "durable transaction staging failed: {error}"
                        )));
                    }
                }
            }
        }
    }
    state.durability.queued.append(&mut deferred);
    submit_dirty_checkpoints(state);
    fail_if_durability_failed(state)
}

fn fail_if_durability_failed(state: &State) -> io::Result<()> {
    if state.durability.failed {
        Err(io::Error::other("durable subsystem failed"))
    } else {
        Ok(())
    }
}

fn durable_request_profile(state: &State, request: &DurableRequest) -> Option<u128> {
    match request {
        DurableRequest::Command { id, .. } | DurableRequest::Pickup { id } => {
            state.clients.get(id).map(|client| client.profile)
        }
        DurableRequest::Expire => None,
    }
}

pub(in crate::server) fn queue_interaction_actions(state: &mut State) {
    if (state.durability.expire_again || state.drops.has_expired())
        && !state.durability.expire_queued
        && state.durability.queued.len() < MAX_DEFERRED_DURABLE_ACTIONS
    {
        state.durability.queued.push_back(DurableRequest::Expire);
        state.durability.expire_queued = true;
        state.durability.expire_again = false;
    }
    let mut ids: Vec<_> = state.clients.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        if state.durability.retry_pickups.contains(&id) {
            continue;
        }
        if state.durability.queued.len() >= MAX_DEFERRED_DURABLE_ACTIONS {
            break;
        }
        state.durability.retry_pickups.insert(id);
        state
            .durability
            .queued
            .push_back(DurableRequest::Pickup { id });
    }
}

fn finish_empty_request(state: &mut State, request: &DurableRequest) {
    match request {
        DurableRequest::Command { id, message, .. } => {
            if let Some(action_id) = command_action_id(message) {
                let profile = state.clients.get(id).map(|client| client.profile);
                let payload = durable::encode_action_receipt(message).ok();
                let receipt =
                    profile.and_then(|profile| state.durability.action_receipt(profile, action_id));
                let accepted = receipt.is_some_and(|receipt| Some(receipt) == payload.as_deref());
                let reason = if accepted {
                    ""
                } else if receipt.is_some() {
                    "action ID was already used for different input"
                } else {
                    "action made no change"
                };
                queue_action_result(state, *id, action_id, accepted, reason);
            }
        }
        DurableRequest::Pickup { id } => {
            state.durability.retry_pickups.remove(id);
        }
        DurableRequest::Expire => {
            state.durability.expire_queued = false;
            state.durability.expire_again = false;
        }
    }
}

fn finish_rejected_request(state: &mut State, request: &DurableRequest, reason: String) {
    match request {
        DurableRequest::Command { id, message, .. } => {
            if let Some(action_id) = command_action_id(message) {
                queue_action_result(state, *id, action_id, false, &reason);
            }
        }
        DurableRequest::Pickup { id } => {
            state.durability.retry_pickups.remove(id);
        }
        DurableRequest::Expire => {
            state.durability.expire_queued = false;
            state.durability.expire_again = false;
        }
    }
}

fn command_action_id(message: &ClientMessage) -> Option<u128> {
    match message {
        ClientMessage::Edit { action_id, .. }
        | ClientMessage::InventoryMove { action_id, .. }
        | ClientMessage::DropStack { action_id, .. } => Some(*action_id),
        _ => None,
    }
}

fn queue_action_result(state: &mut State, id: u64, action_id: u128, accepted: bool, reason: &str) {
    state.durability.publish_queue.push(PublishEffects {
        client_id: Some(id),
        profile: None,
        action_id: Some(action_id),
        accepted,
        reason: short_action_reason(reason),
        inventory: None,
        chunks: Vec::new(),
        deltas: Vec::new(),
        pickups: Vec::new(),
    });
}

fn short_action_reason(reason: &str) -> String {
    let mut reason = reason.to_owned();
    while reason.len() > 32 {
        reason.pop();
    }
    reason
}

#[cfg(test)]
#[path = "coordinator/tests.rs"]
mod tests;

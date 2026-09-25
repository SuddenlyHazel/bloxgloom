//! Tick-thread admission and scheduling for durable gameplay requests.

use super::actions::plan_durable_request;
use super::checkpoint::{process_checkpoint_receipts, submit_dirty_checkpoints};
use super::receipt::poll_journal_receipts;
use super::receipts::{Admission, ReceiptEvent, ReceiptTransition, ResultRecord};
use super::rotation::progress_rotation;
use super::*;
#[cfg(test)]
use crate::server::durable;
use crate::server::{State, handle_message};

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
        | ClientMessage::DropStack { action_id, .. }
        | ClientMessage::AdminGive { action_id, .. }
        | ClientMessage::EntityInteract { action_id, .. } => Some(*action_id),
        _ => None,
    };
    if let Some(action_id) = action_id {
        if state.durability.queued.len() >= MAX_DEFERRED_DURABLE_ACTIONS {
            defer_action(state, id, action_id);
        } else {
            state.durability.queued.push_back(DurableRequest::Command {
                id,
                message,
                queued_at: Instant::now(),
            });
        }
        return Ok(());
    }
    if let ClientMessage::ActionAck { epoch, through_seq } = message {
        if let Some(profile) = state.clients.get(&id).map(|client| client.profile)
            && state.durability.receipt_ledger(profile).current_epoch() == epoch
        {
            state
                .durability
                .pending_acks
                .entry(profile)
                .and_modify(|pending| pending.1 = pending.1.max(through_seq))
                .or_insert((epoch, through_seq));
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
    if let Err(error) = state.durability.entity_mirror.check_health() {
        state.durability.failed = true;
        return Err(error);
    }
    process_checkpoint_receipts(state, now);
    poll_journal_receipts(state)?;
    if state.durability.failed {
        return Err(io::Error::other("durable subsystem failed"));
    }
    if progress_rotation(state)? {
        submit_dirty_checkpoints(state);
        return fail_if_durability_failed(state);
    }

    // ACKs have their own bounded lane. They must be able to retire a full
    // per-profile window even when ordinary durable requests fill their queue.
    let mut ack_profiles: Vec<_> = state.durability.pending_acks.keys().copied().collect();
    ack_profiles.sort_unstable();
    for profile in ack_profiles {
        let Some((epoch, through_seq)) = state.durability.pending_acks.get(&profile).copied()
        else {
            continue;
        };
        match state
            .durability
            .stage_action_ack(profile, epoch, through_seq, tick)
        {
            Ok(_) | Err(StageError::Invalid(_)) => {
                state.durability.pending_acks.remove(&profile);
            }
            Err(StageError::Conflict | StageError::Full) => {}
            Err(error) => return fatal_stage_error(state, error),
        }
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
        let command_receipt = if let DurableRequest::Command { id, message, .. } = &request {
            let Some(profile) = state.clients.get(id).map(|client| client.profile) else {
                continue;
            };
            let action_id = command_action_id(message).expect("queued durable command");
            let payload =
                super::state::encode_action_receipt_with_catalog(message, state.world.catalog())?;
            match state
                .durability
                .receipt_ledger(profile)
                .admission(action_id, &payload)
            {
                Admission::New => Some((profile, action_id, payload)),
                Admission::Replay(record) => {
                    queue_action_result(state, *id, action_id, record.accepted, &record.reason);
                    continue;
                }
                Admission::Retired => {
                    queue_action_result(state, *id, action_id, false, "action already retired");
                    continue;
                }
                Admission::WrongEpoch => {
                    queue_action_result(state, *id, action_id, false, "stale action session");
                    continue;
                }
                Admission::Gap | Admission::Full => {
                    defer_action(state, *id, action_id);
                    continue;
                }
            }
        } else {
            None
        };
        if matches!(
            &request,
            DurableRequest::Command { queued_at, .. }
                if queued_at.elapsed() > Duration::from_secs(10)
        ) {
            let (profile, action_id, payload) = command_receipt.expect("command admission");
            let action = rejected_action(
                state,
                &request,
                profile,
                action_id,
                payload,
                "target state unavailable",
            );
            match state.durability.try_stage(tick, &action, None) {
                Ok(true) => {
                    blocked_profiles.insert(profile);
                }
                Ok(false) => return Err(io::Error::other("empty durable rejection")),
                Err(StageError::Conflict | StageError::Full) => {
                    blocked_profiles.insert(profile);
                    deferred.push_back(request);
                }
                Err(error) => return fatal_stage_error(state, error),
            }
            continue;
        }
        let plan = match plan_durable_request(state, &request, tick) {
            Ok(Some(plan)) => PlannedAction::Commit(Box::new(plan)),
            Ok(None) => PlannedAction::NoChange,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if let Some(profile) = request_profile {
                    blocked_profiles.insert(profile);
                }
                deferred.push_back(request);
                continue;
            }
            Err(error) if error.kind() == ErrorKind::InvalidData => {
                state.durability.failed = true;
                return Err(error);
            }
            Err(error) => PlannedAction::Reject(error.to_string()),
        };
        match plan {
            PlannedAction::NoChange | PlannedAction::Reject(_) | PlannedAction::Commit(_) => {
                let mut action = match plan {
                    PlannedAction::Commit(action) => *action,
                    PlannedAction::NoChange => {
                        if let Some((profile, action_id, payload)) = command_receipt.clone() {
                            rejected_action(
                                state,
                                &request,
                                profile,
                                action_id,
                                payload,
                                "action made no change",
                            )
                        } else {
                            finish_noncommand_request(state, &request);
                            continue;
                        }
                    }
                    PlannedAction::Reject(reason) => {
                        if let Some((profile, action_id, payload)) = command_receipt.clone() {
                            rejected_action(state, &request, profile, action_id, payload, &reason)
                        } else {
                            finish_noncommand_request(state, &request);
                            continue;
                        }
                    }
                };
                if let Some((profile, _, payload)) = command_receipt {
                    if action.receipt_transition.is_none() {
                        if action.receipt_value.as_deref() != Some(payload.as_slice()) {
                            cancel_prepared_entities(state, &action);
                            return Err(io::Error::other(
                                "planned action payload differs from admitted command",
                            ));
                        }
                        let accepted = action.inventory.is_some()
                            || !action.world_edits.is_empty()
                            || action.entities.is_some();
                        let reason = if accepted {
                            String::new()
                        } else {
                            "action made no change".into()
                        };
                        if let Err(error) = set_action_result(
                            state,
                            &mut action,
                            profile,
                            payload,
                            accepted,
                            reason,
                        ) {
                            cancel_prepared_entities(state, &action);
                            return Err(error);
                        }
                    }
                }
                if let Some(entities) = &action.entities {
                    if let Err(error) = state.entities.validate_prepared(entities) {
                        cancel_prepared_entities(state, &action);
                        return Err(io::Error::new(ErrorKind::InvalidData, error));
                    }
                }
                let entity_permit = if action.entities.is_some() {
                    match state.durability.entity_mirror.try_reserve_durable() {
                        Err(error) => {
                            cancel_prepared_entities(state, &action);
                            state.durability.failed = true;
                            return Err(error);
                        }
                        Ok(Some(permit)) => Some(permit),
                        Ok(None) => {
                            cancel_prepared_entities(state, &action);
                            if let Some(profile) = request_profile {
                                blocked_profiles.insert(profile);
                            }
                            deferred.push_back(request);
                            continue;
                        }
                    }
                } else {
                    None
                };
                match state.durability.try_stage(tick, &action, entity_permit) {
                    Ok(true) => {
                        if let Some(seed) = &action.fire_seed
                            && let Err(error) = state.fire.mark_seed_submitted(seed)
                        {
                            // The WAL accepted the transaction. Any failed
                            // in-memory reservation is fatal until replay.
                            state.durability.failed = true;
                            return Err(io::Error::other(format!(
                                "accepted fire seed could not be reserved: {error}"
                            )));
                        }
                        if let Some(profile) = action.profile {
                            blocked_profiles.insert(profile);
                        }
                    }
                    Ok(false) => {
                        cancel_prepared_entities(state, &action);
                        return Err(io::Error::other("empty durable result"));
                    }
                    Err(StageError::Conflict | StageError::Full) => {
                        cancel_prepared_entities(state, &action);
                        if let Some(profile) = request_profile {
                            blocked_profiles.insert(profile);
                        }
                        deferred.push_back(request);
                    }
                    Err(StageError::Closed) => {
                        cancel_prepared_entities(state, &action);
                        state.durability.failed = true;
                        return Err(io::Error::other("durability writer unavailable"));
                    }
                    Err(StageError::IdExhausted) => {
                        cancel_prepared_entities(state, &action);
                        state.durability.failed = true;
                        return Err(io::Error::other("durable transaction IDs exhausted"));
                    }
                    Err(StageError::Invalid(error)) => {
                        cancel_prepared_entities(state, &action);
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

fn cancel_prepared_entities(state: &mut State, action: &CommitAction) {
    if let Some(entities) = &action.entities {
        state.entities.cancel_prepared(entities);
    }
}

fn durable_request_profile(state: &State, request: &DurableRequest) -> Option<u128> {
    match request {
        DurableRequest::Command { id, .. } | DurableRequest::Pickup { id } => {
            state.clients.get(id).map(|client| client.profile)
        }
        DurableRequest::Expire
        | DurableRequest::EntityTick { .. }
        | DurableRequest::EntityWake { .. } => None,
    }
}

pub(in crate::server) fn queue_interaction_actions(state: &mut State, tick: TickId) {
    if state.last_expiry_scan.elapsed() >= Duration::from_secs(1) {
        state.last_expiry_scan = Instant::now();
        if (state.durability.expire_again
            || crate::server::drops::has_expired(&state.entities, crate::server::drops::unix_ms()))
            && !state.durability.expire_queued
            && state.durability.queued.len() < MAX_DEFERRED_DURABLE_ACTIONS
        {
            state.durability.queued.push_back(DurableRequest::Expire);
            state.durability.expire_queued = true;
            state.durability.expire_again = false;
        }
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
    queue_due_entity_ticks(state, tick);
    queue_woken_entity_ticks(state);
}

fn queue_due_entity_ticks(state: &mut State, tick: TickId) {
    let available = MAX_DEFERRED_DURABLE_ACTIONS.saturating_sub(state.durability.queued.len());
    let scan_limit = available.min(MAX_PENDING_DURABLE_ACTIONS);
    if scan_limit == 0 {
        return;
    }
    let mut queued_ids: HashSet<_> = state
        .durability
        .queued
        .iter()
        .filter_map(|request| match request {
            DurableRequest::EntityTick { id } | DurableRequest::EntityWake { id } => Some(*id),
            _ => None,
        })
        .collect();
    for pending in &state.durability.pending {
        if let PendingPayload::Action(action) = &pending.payload
            && let Some(entities) = &action.entities
        {
            queued_ids.extend(entities.entity_ids());
        }
    }
    let due = state.entities.due_tick_entries(
        tick.get(),
        state.durability.entity_tick_cursor,
        scan_limit,
    );
    for (due_tick, id) in due {
        state.durability.entity_tick_cursor = Some((due_tick, id));
        if queued_ids.insert(id) {
            state
                .durability
                .queued
                .push_back(DurableRequest::EntityTick { id });
        }
    }
}

/// Releases committed wakes as transient tick attempts. This runs at the
/// interaction/commit barrier, after the durable phase planned and staged
/// the producer: a wake committed during tick N is queued here and planned
/// no earlier than tick N+1, so delivery never cascades within a tick.
/// Already-scheduled entities collapse duplicate wakes (timing only), and
/// overflow waits for the next barrier instead of dropping or erroring.
fn queue_woken_entity_ticks(state: &mut State) {
    if state.durability.pending_wakes.is_empty() {
        return;
    }
    let available = MAX_DEFERRED_DURABLE_ACTIONS.saturating_sub(state.durability.queued.len());
    if available == 0 {
        return;
    }
    let mut queued_ids: HashSet<_> = state
        .durability
        .queued
        .iter()
        .filter_map(|request| match request {
            DurableRequest::EntityTick { id } | DurableRequest::EntityWake { id } => Some(*id),
            _ => None,
        })
        .collect();
    for pending in &state.durability.pending {
        if let PendingPayload::Action(action) = &pending.payload
            && let Some(entities) = &action.entities
        {
            queued_ids.extend(entities.entity_ids());
        }
    }
    let wakes = std::mem::take(&mut state.durability.pending_wakes);
    let mut remaining = available;
    let mut leftover = Vec::new();
    for id in wakes {
        if remaining == 0 {
            leftover.push(id);
            continue;
        }
        if !queued_ids.insert(id) {
            continue;
        }
        state
            .durability
            .queued
            .push_back(DurableRequest::EntityWake { id });
        remaining -= 1;
    }
    state.durability.pending_wakes = leftover;
}

fn finish_noncommand_request(state: &mut State, request: &DurableRequest) {
    match request {
        DurableRequest::Command { .. } => {}
        DurableRequest::Pickup { id } => {
            state.durability.retry_pickups.remove(id);
        }
        DurableRequest::Expire => {
            state.durability.expire_queued = false;
            state.durability.expire_again = false;
        }
        DurableRequest::EntityTick { .. } | DurableRequest::EntityWake { .. } => {}
    }
}

fn command_action_id(message: &ClientMessage) -> Option<u128> {
    match message {
        ClientMessage::Edit { action_id, .. }
        | ClientMessage::InventoryMove { action_id, .. }
        | ClientMessage::DropStack { action_id, .. }
        | ClientMessage::AdminGive { action_id, .. }
        | ClientMessage::EntityInteract { action_id, .. } => Some(*action_id),
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
        deltas: Vec::new(),
        entity_commit: None,
        pickups: Vec::new(),
    });
}

fn defer_action(state: &mut State, id: u64, action_id: u128) {
    if let Some(client) = state.clients.get(&id) {
        client.enqueue(crate::protocol::ServerMessage::ActionDeferred { action_id });
    }
}

fn rejected_action(
    state: &State,
    request: &DurableRequest,
    profile: u128,
    action_id: u128,
    payload: Vec<u8>,
    reason: &str,
) -> CommitAction {
    let id = match request {
        DurableRequest::Command { id, .. } => *id,
        _ => unreachable!(),
    };
    let mut action = CommitAction::receipt_only(
        ReceiptTransition::new(
            profile,
            &state.durability.receipt_ledger(profile),
            state
                .durability
                .receipt_ledger(profile)
                .append_result(ResultRecord {
                    payload: payload.clone(),
                    accepted: false,
                    reason: short_action_reason(reason),
                })
                .expect("admitted result"),
            ReceiptEvent::Result(ResultRecord {
                payload,
                accepted: false,
                reason: short_action_reason(reason),
            }),
        )
        .expect("valid admitted receipt transition"),
    );
    action.client_id = Some(id);
    action.action_id = Some(action_id);
    action
}

fn set_action_result(
    state: &State,
    action: &mut CommitAction,
    profile: u128,
    payload: Vec<u8>,
    accepted: bool,
    reason: String,
) -> io::Result<()> {
    let before = state.durability.receipt_ledger(profile);
    let record = ResultRecord {
        payload,
        accepted,
        reason: short_action_reason(&reason),
    };
    let after = before.append_result(record.clone())?;
    action.receipt_transition = Some(ReceiptTransition::new(
        profile,
        &before,
        after,
        ReceiptEvent::Result(record),
    )?);
    Ok(())
}

fn fatal_stage_error(state: &mut State, error: StageError) -> io::Result<()> {
    state.durability.failed = true;
    Err(io::Error::other(format!(
        "durable action staging failed: {error:?}"
    )))
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

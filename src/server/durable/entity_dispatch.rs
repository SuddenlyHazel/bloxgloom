//! Bounded entity policy dispatch. The coordinator alone captures authority
//! and constructs transactions; workers only execute immutable tick policies.

use super::actions::entity::{self, TickWorkerResult};
use super::*;
use crate::server::State;
use crate::server::parallel::{BatchId, JobKey, JobOutcome, OwnerKey, SubmitError};
use crate::server::simulation::Phase;

pub(super) fn plan_motion(
    state: &mut State,
    tick: TickId,
    requests: Vec<DurableRequest>,
) -> Vec<(DurableRequest, io::Result<Option<CommitAction>>)> {
    let wave = match state.entity_tick_dispatch_batch {
        Some((last_tick, wave)) if last_tick == tick => wave.checked_add(1),
        _ => Some(0),
    };
    let Some(wave) = wave else {
        return requests
            .into_iter()
            .map(|request| {
                (
                    request,
                    Err(io::Error::new(
                        ErrorKind::WouldBlock,
                        "entity tick dispatch waves exhausted",
                    )),
                )
            })
            .collect();
    };
    state.entity_tick_dispatch_batch = Some((tick, wave));
    let batch = BatchId::new(tick, Phase::DurableActions, wave);
    let mut results: Vec<Option<io::Result<Option<CommitAction>>>> =
        (0..requests.len()).map(|_| None).collect();
    let mut completed: Vec<
        Option<JobOutcome<TickWorkerResult, crate::server::entities::EntityError>>,
    > = (0..requests.len()).map(|_| None).collect();
    for (index, request) in requests.iter().enumerate() {
        let (id, woken) = match request {
            DurableRequest::EntityTick { id } => (*id, false),
            DurableRequest::EntityWake { id } => (*id, true),
            _ => unreachable!("only entity ticks enter motion dispatch"),
        };
        let input = match entity::capture_tick_input(state, id, tick.get(), woken) {
            Ok(Some(input)) => input,
            Ok(None) => {
                results[index] = Some(Ok(None));
                continue;
            }
            Err(error) => {
                results[index] = Some(Err(error));
                continue;
            }
        };
        let key = JobKey::new(
            batch,
            OwnerKey::Entity(id.get()),
            index as u64,
            input.snapshot.revision,
        );
        match state.entity_tick_executor.try_submit(key, move |_| {
            let plan = input.plan()?;
            Ok(TickWorkerResult { input, plan })
        }) {
            Ok(()) => {}
            Err(SubmitError::QueueSaturated { .. }) => {
                results[index] = Some(Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "entity tick worker queue full",
                )));
            }
            Err(_) => {
                results[index] = Some(Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "entity tick worker unavailable",
                )));
            }
        }
    }
    // A failed barrier leaves all accepted work retryable; no partial apply.
    if let Ok(completions) = state.entity_tick_executor.barrier(batch) {
        // PhaseExecutor orders by owner; dispatch must restore original
        // request order, including repeated wakes for one entity.
        for owner in completions.owners {
            for completion in owner.jobs {
                let index = completion.key.job_id as usize;
                completed[index] = Some(completion.outcome);
            }
        }
    }
    for (index, outcome) in completed.into_iter().enumerate() {
        if let Some(outcome) = outcome {
            results[index] = Some(match outcome {
                JobOutcome::Completed(result) => {
                    entity::commit_tick_plan(state, result.input, result.plan)
                }
                JobOutcome::Failed(error) => Err(io::Error::new(ErrorKind::InvalidInput, error)),
                JobOutcome::Panicked(_) | JobOutcome::Cancelled | JobOutcome::Stale => Err(
                    io::Error::new(ErrorKind::WouldBlock, "entity tick worker did not complete"),
                ),
            });
        }
    }
    requests
        .into_iter()
        .enumerate()
        .map(|(index, request)| {
            (
                request,
                results[index].take().unwrap_or_else(|| {
                    Err(io::Error::new(
                        ErrorKind::WouldBlock,
                        "entity tick worker barrier unavailable",
                    ))
                }),
            )
        })
        .collect()
}

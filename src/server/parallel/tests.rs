use super::super::effects::{
    Effect, EffectBatch, EffectBuffer, EffectEnvelope, EffectLimits, route_effects,
};
use super::*;
use crate::world::world_to_chunk;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::time::Duration;

fn owner_at(position: [i32; 3]) -> ChunkKey {
    world_to_chunk(position[0], position[1], position[2]).0
}

fn key(tick: u64, owner: ChunkKey, job_id: u64, revision: u64) -> JobKey {
    JobKey::new(batch(tick, 0), owner, job_id, revision)
}

fn batch(tick: u64, wave: u16) -> BatchId {
    BatchId::new(TickId::new(tick), Phase::Simulation, wave)
}

#[test]
fn batch_id_exposes_tick_phase_and_dependency_wave() {
    let batch = BatchId::new(TickId::new(13), Phase::InteractionCommit, 2);

    assert_eq!(batch.tick().get(), 13);
    assert_eq!(batch.phase(), Phase::InteractionCommit);
    assert_eq!(batch.wave(), 2);
}

#[test]
fn phase_results_report_worker_count_and_nonzero_closure_time() {
    let batch = batch(1, 0);
    let mut executor = PhaseExecutor::<(), ()>::new(2, 2, 2).unwrap();
    assert_eq!(executor.worker_count(), 2);
    executor
        .try_submit(key(1, owner_at([0, 0, 0]), 1, 0), |_| {
            thread::sleep(Duration::from_millis(2));
            Ok(())
        })
        .unwrap();

    let results = executor.barrier(batch).unwrap();
    assert!(!results.worker_run_time().is_zero());
}

#[test]
fn pre_cancelled_job_has_zero_run_time_and_never_invokes_its_closure() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let invocations = Arc::new(AtomicUsize::new(0));
    let closure_invocations = Arc::clone(&invocations);
    let run: PhaseJob<(), ()> = Box::new(move |_| {
        closure_invocations.fetch_add(1, AtomicOrdering::Relaxed);
        Ok(())
    });

    let (outcome, run_time) = execute_task(cancellation, run);

    assert!(matches!(outcome, WorkerOutcome::Cancelled));
    assert_eq!(run_time, Duration::ZERO);
    assert_eq!(invocations.load(AtomicOrdering::Relaxed), 0);
}

fn completed_jobs<R, E>(results: &PhaseResults<R, E>) -> usize {
    results.owners().iter().map(|owner| owner.jobs.len()).sum()
}

fn run_scheduled(reverse_submission: bool, delay_offset: u64) -> (Vec<(JobKey, u8)>, usize, usize) {
    let batch = batch(1, 0);
    let snapshot: Arc<[u8]> = Arc::from([13, 23, 37, 41, 53, 67, 79, 83]);
    let gate = Arc::new(std::sync::Barrier::new(3));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum_active = Arc::new(AtomicUsize::new(0));
    let specs = [
        (4, [15, 0, 0], 7, 1),
        (3, [16, 0, 0], 8, 4),
        (9, [-1, -1, -1], 9, 2),
        (2, [-16, 0, -1], 4, 6),
        (5, [32, -16, -16], 2, 3),
        (1, [0, 16, 0], 3, 7),
        (7, [4, 0, 0], 1, 5),
    ];
    let mut ordered_specs: Vec<_> = specs.into_iter().collect();
    if reverse_submission {
        ordered_specs.reverse();
    }

    let mut executor = PhaseExecutor::<(u8, String), ()>::new(3, 8, 8).unwrap();
    for (submission, (job_id, position, revision, delay)) in ordered_specs.into_iter().enumerate() {
        let snapshot = Arc::clone(&snapshot);
        let gate = Arc::clone(&gate);
        let active = Arc::clone(&active);
        let maximum_active = Arc::clone(&maximum_active);
        let wait = Duration::from_millis(delay + delay_offset);
        let result_index = job_id as usize % snapshot.len();
        executor
            .try_submit(key(1, owner_at(position), job_id, revision), move |_| {
                if submission < 3 {
                    gate.wait();
                }
                let now_active = active.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                maximum_active.fetch_max(now_active, AtomicOrdering::Relaxed);
                thread::sleep(wait);
                active.fetch_sub(1, AtomicOrdering::Relaxed);
                Ok((
                    snapshot[result_index],
                    format!("{:?}", thread::current().id()),
                ))
            })
            .unwrap();
    }

    let results = executor.barrier(batch).unwrap();
    let mut thread_ids = HashSet::new();
    let mut stable = Vec::new();
    for owner_results in results.owners() {
        for job in &owner_results.jobs {
            let JobOutcome::Completed((value, thread_id)) = &job.outcome else {
                panic!("unexpected job outcome: {:?}", job.outcome);
            };
            thread_ids.insert(thread_id.clone());
            stable.push((job.key, *value));
        }
    }
    (
        stable,
        maximum_active.load(AtomicOrdering::Relaxed),
        thread_ids.len(),
    )
}

fn run_effect_barrier(worker_count: usize, reverse_submission: bool) -> EffectBatch {
    let tick = TickId::new(13);
    let batch = BatchId::new(tick, Phase::Simulation, 0);
    let jobs = [
        (
            1,
            [4, 2, 2],
            Effect::BlockChanged {
                cell: crate::server::effects::CellCoord::new(15, 2, 2),
            },
            4,
        ),
        (
            2,
            [5, 2, 2],
            Effect::WakeDrop {
                id: 44,
                owner: owner_at([5, 2, 2]),
            },
            1,
        ),
        (
            7,
            [16, 2, 2],
            Effect::BlockChanged {
                cell: crate::server::effects::CellCoord::new(16, 2, 2),
            },
            2,
        ),
        (
            3,
            [-16, 2, 2],
            Effect::BlockChanged {
                cell: crate::server::effects::CellCoord::new(-16, 2, 2),
            },
            3,
        ),
    ];
    let mut submitted: Vec<_> = jobs.into_iter().collect();
    if reverse_submission {
        submitted.reverse();
    }

    let mut executor =
        PhaseExecutor::<Vec<EffectEnvelope>, String>::new(worker_count, 8, 8).unwrap();
    for (job_id, owner_position, effect, delay) in submitted {
        let owner = owner_at(owner_position);
        executor
            .try_submit(key(13, owner, job_id, 5), move |_| {
                thread::sleep(Duration::from_millis(delay));
                let mut output = EffectBuffer::new(tick, job_id, 1)
                    .map_err(|error| format!("buffer: {error:?}"))?;
                output
                    .emit(effect)
                    .map_err(|error| format!("emit: {error:?}"))?;
                output
                    .finish()
                    .map_err(|error| format!("finish: {error:?}"))
            })
            .unwrap();
    }

    let results = executor.barrier(batch).unwrap();
    let effects = results
        .owners()
        .iter()
        .flat_map(|owner| &owner.jobs)
        .flat_map(|job| match &job.outcome {
            JobOutcome::Completed(effects) => effects.clone(),
            other => panic!("unexpected phase job result: {other:?}"),
        })
        .collect::<Vec<_>>();
    route_effects(effects, EffectLimits::default()).unwrap()
}

#[test]
fn owner_commits_and_cross_owner_effect_routes_match_with_one_and_four_workers() {
    let single_worker = run_effect_barrier(1, false);
    let four_workers = run_effect_barrier(4, true);

    assert_eq!(single_worker, four_workers);
    assert_eq!(
        single_worker.commit_phase(),
        super::super::simulation::Phase::InteractionCommit
    );
    let local_owner = owner_at([4, 2, 2]);
    let local = single_worker
        .owners()
        .iter()
        .find(|owner| owner.owner == local_owner)
        .unwrap();
    assert_eq!(local.effects.len(), 3);
    assert!(
        single_worker
            .owners()
            .iter()
            .any(|owner| owner.owner == owner_at([16, 2, 2]))
    );
    assert!(
        single_worker
            .owners()
            .iter()
            .any(|owner| owner.owner == owner_at([-16, 2, 2]))
    );
}

#[test]
fn owner_results_are_stable_across_regions_and_completion_orders() {
    let (first, first_concurrency, first_threads) = run_scheduled(false, 0);
    let (second, second_concurrency, second_threads) = run_scheduled(true, 3);
    assert_eq!(first, second);
    assert_eq!(first.len(), 7);
    assert!(first_concurrency >= 2);
    assert!(second_concurrency >= 2);
    assert!(first_threads <= 3);
    assert!(second_threads <= 3);

    for pair in first.windows(2) {
        assert!(pair[0].0 < pair[1].0);
    }
    assert!(first.iter().any(|(key, _)| key.owner.x < 0));
    assert!(first.iter().any(|(key, _)| key.owner.x > 0));
}

#[test]
fn bounded_queue_reports_saturation_without_accepting_a_partial_job() {
    let batch = batch(2, 0);
    let owner = owner_at([0, 0, 0]);
    let mut executor = PhaseExecutor::<u8, ()>::new(1, 1, 2).unwrap();
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();

    executor
        .try_submit(key(2, owner, 1, 0), move |_| {
            started_sender.send(()).unwrap();
            release_receiver.recv().unwrap();
            Ok(1)
        })
        .unwrap();
    started_receiver.recv().unwrap();
    executor.try_submit(key(2, owner, 2, 0), |_| Ok(2)).unwrap();

    assert_eq!(
        executor.try_submit(key(2, owner, 3, 0), |_| Ok(3)),
        Err(SubmitError::QueueSaturated {
            key: key(2, owner, 3, 0),
            capacity: 1,
        })
    );
    release_sender.send(()).unwrap();

    let results = executor.barrier(batch).unwrap();
    assert_eq!(completed_jobs(&results), 2);
    assert_eq!(
        results.owners()[0].jobs[0].outcome,
        JobOutcome::Completed(1)
    );
    assert_eq!(
        results.owners()[0].jobs[1].outcome,
        JobOutcome::Completed(2)
    );
}

#[test]
fn cancellation_skips_queued_work_and_marks_running_results_cancelled() {
    let batch = batch(5, 0);
    let owner = owner_at([-1, 0, 0]);
    let mut executor = PhaseExecutor::<u8, ()>::new(1, 1, 2).unwrap();
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let queued_invocations = Arc::new(AtomicUsize::new(0));

    executor
        .try_submit(key(5, owner, 1, 4), move |_| {
            started_sender.send(()).unwrap();
            release_receiver.recv().unwrap();
            Ok(1)
        })
        .unwrap();
    started_receiver.recv().unwrap();
    let queued_invocations_job = Arc::clone(&queued_invocations);
    executor
        .try_submit(key(5, owner, 2, 4), move |_| {
            queued_invocations_job.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(2)
        })
        .unwrap();

    executor.cancel_batch(batch).unwrap();
    release_sender.send(()).unwrap();
    let results = executor.barrier(batch).unwrap();
    assert_eq!(completed_jobs(&results), 2);
    assert!(
        results.owners()[0]
            .jobs
            .iter()
            .all(|job| job.outcome == JobOutcome::Cancelled)
    );
    assert_eq!(queued_invocations.load(AtomicOrdering::Relaxed), 0);
}

#[test]
fn stale_snapshot_results_are_discarded_at_the_barrier() {
    let batch = batch(8, 0);
    let mut executor = PhaseExecutor::<u16, ()>::new(2, 4, 4).unwrap();
    let current_owner = owner_at([0, 0, 0]);
    let stale_owner = owner_at([16, 0, 0]);
    executor
        .try_submit(key(8, current_owner, 1, 12), |_| Ok(120))
        .unwrap();
    executor
        .try_submit(key(8, stale_owner, 2, 11), |_| Ok(110))
        .unwrap();

    let results = executor
        .barrier_with(batch, |job_key| job_key.snapshot_revision == 12)
        .unwrap();
    assert_eq!(completed_jobs(&results), 2);
    assert_eq!(
        results.owners()[0].jobs[0].outcome,
        JobOutcome::Completed(120)
    );
    assert_eq!(results.owners()[1].jobs[0].outcome, JobOutcome::Stale);
}

#[test]
fn job_errors_and_panics_reach_the_barrier_and_the_pool_keeps_running() {
    let mut executor = PhaseExecutor::<u8, &'static str>::new(2, 4, 4).unwrap();
    let owner = owner_at([0, 0, 0]);
    executor
        .try_submit(key(10, owner, 1, 1), |_| Err("bad input"))
        .unwrap();
    executor
        .try_submit(key(10, owner, 2, 1), |_| -> Result<u8, &'static str> {
            panic!("worker job failed")
        })
        .unwrap();
    let first = executor.barrier(batch(10, 0)).unwrap();
    assert_eq!(
        first.owners()[0].jobs[0].outcome,
        JobOutcome::Failed("bad input")
    );
    assert_eq!(
        first.owners()[0].jobs[1].outcome,
        JobOutcome::Panicked("worker job failed".into())
    );

    executor
        .try_submit(key(11, owner, 1, 2), |_| Ok(7))
        .unwrap();
    let next = executor.barrier(batch(11, 0)).unwrap();
    assert_eq!(next.owners()[0].jobs[0].outcome, JobOutcome::Completed(7));
}

#[test]
fn revisions_and_job_ids_participate_in_keys_and_batch_barriers_close_submission() {
    let mut executor = PhaseExecutor::<u8, ()>::new(1, 2, 2).unwrap();
    let owner = owner_at([0, 0, 0]);
    executor.try_submit(key(1, owner, 4, 3), |_| Ok(4)).unwrap();
    assert_eq!(
        executor.try_submit(key(1, owner, 4, 3), |_| Ok(99)),
        Err(SubmitError::DuplicateKey {
            key: key(1, owner, 4, 3)
        })
    );
    executor.barrier(batch(1, 0)).unwrap();
    assert_eq!(
        executor.try_submit(key(1, owner, 5, 3), |_| Ok(5)),
        Err(SubmitError::ClosedBatch {
            key: key(1, owner, 5, 3),
            closed_through: batch(1, 0),
        })
    );
    assert_eq!(
        executor.barrier(batch(1, 0)),
        Err(BarrierError::AlreadyClosed {
            batch: batch(1, 0),
            closed_through: batch(1, 0),
        })
    );
}

#[test]
fn dependency_waves_can_commit_twice_in_one_tick_and_later_wave_sees_prior_result() {
    let owner = owner_at([-1, 0, 0]);
    let first_batch = batch(14, 0);
    let second_batch = batch(14, 1);
    let mut executor = PhaseExecutor::<u32, ()>::new(2, 4, 4).unwrap();

    executor
        .try_submit(key(14, owner, 1, 1), |_| Ok(41))
        .unwrap();
    let second_key = JobKey::new(second_batch, owner, 2, 2);
    assert_eq!(
        executor.try_submit(second_key, |_| Ok(999)),
        Err(SubmitError::EarlierBatchPending {
            key: second_key,
            pending: first_batch,
        })
    );

    let first_commit = executor.barrier(first_batch).unwrap();
    assert_eq!(first_commit.batch, first_batch);
    let committed_value = match first_commit.owners()[0].jobs[0].outcome {
        JobOutcome::Completed(value) => value,
        ref other => panic!("unexpected wave 0 outcome: {other:?}"),
    };

    executor
        .try_submit(second_key, move |_| Ok(committed_value + 1))
        .unwrap();
    let second_commit = executor.barrier(second_batch).unwrap();
    assert_eq!(second_commit.batch, second_batch);
    assert_eq!(
        second_commit.owners()[0].jobs[0].outcome,
        JobOutcome::Completed(42)
    );
    assert_eq!(first_batch.tick(), second_batch.tick());
    assert!(matches!(
        executor.try_submit(key(14, owner, 3, 1), |_| Ok(0)),
        Err(SubmitError::ClosedBatch {
            closed_through,
            ..
        }) if closed_through == second_batch
    ));
}

#[test]
fn executor_configuration_rejects_unbounded_or_empty_worker_resources() {
    assert!(matches!(
        PhaseExecutor::<(), ()>::new(0, 1, 1),
        Err(ExecutorConfigError::NoWorkers)
    ));
    assert!(matches!(
        PhaseExecutor::<(), ()>::new(1, 0, 1),
        Err(ExecutorConfigError::ZeroQueueCapacity)
    ));
    assert!(matches!(
        PhaseExecutor::<(), ()>::new(1, 1, 0),
        Err(ExecutorConfigError::ZeroResultCapacity)
    ));
}

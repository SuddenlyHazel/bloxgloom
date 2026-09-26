//! Registered dispatch, scheduling, and WAL recovery regressions.
use super::*;
use crate::server::parallel::OwnerSchedule;
use crate::server::startup::{ServerStartup, StartupOwnerCodec};

fn increment(job: &OwnerJob) -> OwnerPatch {
    let value = job
        .snapshot(job.owner())
        .unwrap()
        .value::<OwnerData>()
        .unwrap()
        .get::<u64>()
        .unwrap();
    OwnerPatch::new(
        job,
        OwnerData::new(value + 1),
        PatchUsage {
            writes: 1,
            effects: 0,
            estimated_bytes: 8,
        },
    )
}

fn scheduled_startup(jobs: usize, owners: i32) -> (ServerStartup, SystemId) {
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    let system = SystemId::new("test:scheduled_counter").unwrap();
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Chunk,
            jobs,
            0,
        )
        .write(ResourceId::new("test:scheduled_state").unwrap()),
        |job: &OwnerJob| {
            let tick = job.key().batch.tick().get();
            Ok(
                increment(job).with_schedule(OwnerSchedule::AtTick(if tick < 20 {
                    20
                } else {
                    tick + 10
                })),
            )
        },
    );
    startup.register_owner_codec(
        system.clone(),
        StartupOwnerCodec {
            codec: Arc::new(U64OwnerCodec),
            codec_version: 1,
            max_bytes: 8,
        },
    );
    for x in 0..owners {
        startup.seed_owner(system.clone(), chunk_owner(x), 0u64);
    }
    (startup, system)
}

fn run(state: &mut crate::server::State, system: &SystemId, tick: u64) -> usize {
    let executable = state.phase_plan.system(system).unwrap().clone();
    state
        .system_runtime
        .run_registered(
            &executable,
            TickId::new(tick),
            0,
            &state.effect_kinds,
            &mut state.durability,
        )
        .unwrap()
}

#[test]
fn multi_job_due_dispatch_and_restart_keep_deadlines_and_rotation() {
    let save = TestSave::new();
    let (startup, system) = scheduled_startup(4, 17);
    let mut state =
        crate::server::server_state_with_startup(7, save.0.clone(), 2, startup).unwrap();
    // Real handlers, not fixture-injected schedules, establish all deadlines.
    for tick in 1..=5 {
        assert_eq!(
            run(&mut state, &system, tick),
            if tick == 5 { 1 } else { 4 }
        );
    }
    assert_eq!(run(&mut state, &system, 19), 0);
    assert_eq!(
        run(&mut state, &system, 20),
        4,
        "feed must use the whole job allowance"
    );
    for x in 0..17 {
        assert_eq!(
            state
                .system_runtime
                .owner_value::<u64>(&system, chunk_owner(x)),
            Some(if x < 4 { (2, 2) } else { (1, 1) })
        );
    }
    drop(state);
    let (startup, _) = scheduled_startup(4, 17);
    let mut state =
        crate::server::server_state_with_startup(7, save.0.clone(), 2, startup).unwrap();
    // Recovery starts a fresh bounded feed, retaining persisted eligibility
    // and the ordinary cursor, not claiming exact transient ready-set order.
    for tick in 21..=24 {
        assert_eq!(
            run(&mut state, &system, tick),
            if tick == 24 { 1 } else { 4 }
        );
    }
    for x in 0..17 {
        assert_eq!(
            state
                .system_runtime
                .owner_value::<u64>(&system, chunk_owner(x)),
            Some((2, 2))
        );
    }
    assert_eq!(run(&mut state, &system, 29), 0);
    assert_eq!(run(&mut state, &system, 30), 4);
    assert!(!state.durability.failed);
}

#[test]
fn due_reschedule_waits_for_receipt_and_deferral_preserves_eligibility() {
    let save = TestSave::new();
    let (startup, system) = scheduled_startup(1, 1);
    let mut state =
        crate::server::server_state_with_startup(7, save.0.clone(), 1, startup).unwrap();
    assert_eq!(run(&mut state, &system, 1), 1);
    let executable = state.phase_plan.system(&system).unwrap().clone();
    let stage = |state: &mut crate::server::State,
                 wave: u16,
                 conflicts: &[Vec<crate::server::journal::StateKey>]| {
        state.system_runtime.stage_registered_wave(
            &executable,
            TickId::new(20),
            wave,
            &state.effect_kinds,
            &mut state.durability,
            conflicts,
        )
    };
    // A failed admission must not remove the due owner or change its deadline.
    let key = crate::server::runtime::owner_codec::owner_state_key(&system, chunk_owner(0));
    assert_eq!(
        stage(&mut state, 0, &[vec![key]]).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    let pending = stage(&mut state, 1, &[]).unwrap().unwrap();
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&system, chunk_owner(0)),
        Some((1, 1))
    );
    state
        .system_runtime
        .drain_registered_waves(vec![pending], &mut state.durability)
        .unwrap();
    assert_eq!(
        run(&mut state, &system, 21),
        0,
        "stale ready entries must be removed on reschedule"
    );
    drop(state);
    let (startup, _) = scheduled_startup(1, 1);
    let mut state =
        crate::server::server_state_with_startup(7, save.0.clone(), 1, startup).unwrap();
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&system, chunk_owner(0)),
        Some((2, 2))
    );
    assert_eq!(run(&mut state, &system, 29), 0);
    assert_eq!(run(&mut state, &system, 30), 1);
    assert_eq!(run(&mut state, &system, 31), 0);
}

#[test]
fn mixed_active_and_recurring_due_owners_progress_with_and_without_wakes() {
    for wakes in [false, true] {
        let runs = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&runs);
        let mut harness = nudge_harness(
            1,
            1,
            8,
            4,
            false,
            [0; 3],
            PatchUsage::default(),
            move |job: &OwnerJob| {
                recorded.lock().unwrap().push(job.owner());
                let patch = increment(job);
                let value = patch.payload::<OwnerData>().unwrap().clone();
                let effects = if wakes {
                    vec![EmittedOwnerEffect::new(
                        EffectKindId::new("test:nudge").unwrap(),
                        Nudge {
                            to: chunk_owner(2),
                            amount: 1,
                        },
                    )]
                } else {
                    Vec::new()
                };
                let patch = OwnerPatch::new(
                    job,
                    OwnerEffectPatch::new(value, effects),
                    PatchUsage {
                        writes: 1,
                        effects: usize::from(wakes),
                        estimated_bytes: 8,
                    },
                );
                Ok(if job.owner() == chunk_owner(2) {
                    patch.with_schedule(OwnerSchedule::AtTick(job.key().batch.tick().get() + 1))
                } else {
                    patch
                })
            },
        );
        for tick in 1..=18 {
            tick_nudge(&mut harness, tick).unwrap();
        }
        let runs = runs.lock().unwrap();
        if wakes {
            // Every third tick reserves ordinary work; wake-only waves must
            // not reset that rotation even though the high-key owner reschedules.
            assert_eq!(
                runs.iter().step_by(3).copied().collect::<Vec<_>>(),
                [0, 1, 2, 0, 1, 2].map(chunk_owner)
            );
        } else {
            assert_eq!(
                *runs,
                (0..18).map(|x| chunk_owner(x % 3)).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn wake_only_turns_without_runnable_work_preserve_ordinary_rotation() {
    let runs = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&runs);
    let mut harness = nudge_harness(
        1,
        1,
        8,
        4,
        false,
        [0; 3],
        PatchUsage::default(),
        move |job: &OwnerJob| {
            recorded.lock().unwrap().push(job.owner());
            let value = increment(job).payload::<OwnerData>().unwrap().clone();
            let tick = job.key().batch.tick().get();
            Ok(OwnerPatch::new(
                job,
                OwnerEffectPatch::new(
                    value,
                    vec![EmittedOwnerEffect::new(
                        EffectKindId::new("test:nudge").unwrap(),
                        Nudge {
                            to: chunk_owner(1),
                            amount: 1,
                        },
                    )],
                ),
                PatchUsage {
                    writes: 1,
                    effects: 1,
                    estimated_bytes: 8,
                },
            )
            .with_schedule(OwnerSchedule::AtTick(if tick < 39 {
                39
            } else {
                tick + 39
            })))
        },
    );
    for tick in [1, 2, 3, 4, 5, 39, 40] {
        tick_nudge(&mut harness, tick).unwrap();
    }
    // After tick 4 all owners have future deadlines and rotation points at 0.
    // Tick 5 wakes 1 with no ordinary work: it must not move rotation to 2.
    // Tick 39 feeds 0 but serves the wake; tick 40 feeds 2 and serves ordinary
    // work. Both ready entries must be ordered by the original cursor.
    assert_eq!(
        *runs.lock().unwrap(),
        [0, 1, 1, 2, 1, 1, 0].map(chunk_owner)
    );
}

#[test]
fn wake_advances_deadline_work_and_handler_can_return_to_active() {
    let mut harness = nudge_harness(
        2,
        3,
        8,
        4,
        false,
        [0; 3],
        PatchUsage::default(),
        move |job: &OwnerJob| {
            let patch = increment(job);
            let value = *patch.payload::<OwnerData>().unwrap().get::<u64>().unwrap();
            let effects = if job.owner() == chunk_owner(0) && value == 1 {
                vec![EmittedOwnerEffect::new(
                    EffectKindId::new("test:nudge").unwrap(),
                    Nudge {
                        to: chunk_owner(2),
                        amount: 1,
                    },
                )]
            } else {
                Vec::new()
            };
            let count = effects.len();
            let patch = OwnerPatch::new(
                job,
                OwnerEffectPatch::new(OwnerData::new(value), effects),
                PatchUsage {
                    writes: 1,
                    effects: count,
                    estimated_bytes: 8,
                },
            );
            Ok(if value < 3 {
                patch.with_schedule(OwnerSchedule::AtTick(job.key().batch.tick().get() + 10))
            } else {
                patch
            })
        },
    );
    tick_nudge(&mut harness, 1).unwrap();
    tick_nudge(&mut harness, 2).unwrap(); // owner 2 wakes before its deadline 11
    assert_eq!(
        harness
            .state
            .system_runtime
            .owner_value::<u64>(&harness.system, chunk_owner(2)),
        Some((2, 2))
    );
    tick_nudge(&mut harness, 11).unwrap(); // superseded deadline must not run it
    assert_eq!(
        harness
            .state
            .system_runtime
            .owner_value::<u64>(&harness.system, chunk_owner(2)),
        Some((2, 2))
    );
    tick_nudge(&mut harness, 12).unwrap();
    tick_nudge(&mut harness, 13).unwrap(); // default Active resumes ordinary turns
    assert_eq!(
        harness
            .state
            .system_runtime
            .owner_value::<u64>(&harness.system, chunk_owner(2)),
        Some((4, 4))
    );
}

#[test]
fn invalid_handler_deadline_rejects_without_losing_due_work() {
    let invalid = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let handler_invalid = Arc::clone(&invalid);
    let mut harness = nudge_harness(
        1,
        3,
        0,
        0,
        false,
        [0; 3],
        PatchUsage::default(),
        move |job: &OwnerJob| {
            let tick = job.key().batch.tick().get();
            let due = tick + u64::from(!handler_invalid.load(Ordering::SeqCst));
            Ok(increment(job).with_schedule(OwnerSchedule::AtTick(due)))
        },
    );
    let system = harness.system.clone();
    assert_eq!(run(&mut harness.state, &system, 1), 3);
    invalid.store(true, Ordering::SeqCst);
    assert!(tick_nudge(&mut harness, 2).is_err());
    for owner in harness.owners {
        assert_eq!(
            harness
                .state
                .system_runtime
                .owner_value::<u64>(&system, owner),
            Some((1, 1))
        );
    }
    invalid.store(false, Ordering::SeqCst);
    assert_eq!(run(&mut harness.state, &system, 3), 3);
}

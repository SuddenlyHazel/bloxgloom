use super::*;
use crate::server::effects::{EffectConsumerOutput, EffectKindId, EffectKindRegistry};
use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
use crate::server::registry::{
    OwnerPartition, ResourceId, SystemDescriptor, SystemHandler, SystemHandlerError, SystemId,
    SystemRegistry,
};
use crate::server::runtime::owner_codec::{OwnerCodecError, OwnerValueCodec};
use crate::server::runtime::owner_durable::OwnerSystemConfig;
use crate::server::runtime::owner_effects::{EmittedOwnerEffect, OwnerEffectPatch};
use crate::server::runtime::systems::SystemRuntime;
use crate::server::simulation::Phase;
use crate::world::ChunkKey;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread::{self, ThreadId};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[path = "scheduling_tests.rs"]
mod scheduling;

#[path = "commit_lifecycle_tests.rs"]
mod commit_lifecycle;

/// Little-endian u64 owner codec for test harnesses that drive
/// `SystemRuntime` directly. Production systems register their codec through
/// `ServerStartup::register_owner_codec`.
struct U64OwnerCodec;

impl OwnerValueCodec for U64OwnerCodec {
    fn decode(&self, payload: &[u8]) -> Result<OwnerData, OwnerCodecError> {
        if payload.len() != 8 {
            return Err(OwnerCodecError::InvalidData);
        }
        Ok(OwnerData::new(u64::from_le_bytes(
            payload.try_into().expect("checked length"),
        )))
    }

    fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError> {
        value
            .get::<u64>()
            .map(|value| value.to_le_bytes().to_vec())
            .ok_or(OwnerCodecError::InvalidData)
    }
}

fn register_u64_state(runtime: &mut SystemRuntime, system: &SystemId) {
    runtime
        .register_owner_system(
            OwnerSystemConfig::new(
                system.clone(),
                Arc::new(U64OwnerCodec),
                1,
                8,
                OwnerPartition::Chunk,
            )
            .unwrap(),
        )
        .unwrap();
}

struct TestSave(PathBuf);

impl TestSave {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-registered-dispatch-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestSave {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn driverless_startup_handler_runs_on_one_and_four_workers_and_commits() {
    fn run(workers: usize) -> (Vec<(u64, u64)>, usize) {
        let save = TestSave::new();
        let mut state = crate::server::server_state(113, save.0.clone()).unwrap();
        state.system_runtime = SystemRuntime::new(workers).unwrap();
        assert_eq!(state.system_runtime.worker_count(), workers);

        let mut registry = SystemRegistry::new();
        crate::server::builtins::register_builtin_systems(&mut registry).unwrap();
        let rendezvous = Arc::new(Barrier::new(workers));
        let worker_ids = Arc::new(Mutex::new(std::collections::HashSet::<ThreadId>::new()));
        let systems = [
            SystemId::new("test:live_owner_a").unwrap(),
            SystemId::new("test:live_owner_b").unwrap(),
        ];
        for system in &systems {
            let handler_rendezvous = Arc::clone(&rendezvous);
            let handler_worker_ids = Arc::clone(&worker_ids);
            let resource =
                ResourceId::new(format!("test:{}_state", system.as_str().replace(':', "_")))
                    .unwrap();
            registry
                .register_handler(
                    SystemDescriptor::new(
                        system.clone(),
                        Phase::Simulation,
                        OwnerPartition::Chunk,
                        8,
                        0,
                    )
                    .write(resource),
                    move |job: &OwnerJob| {
                        handler_worker_ids
                            .lock()
                            .unwrap()
                            .insert(thread::current().id());
                        handler_rendezvous.wait();
                        let value = job
                            .snapshot(job.owner())
                            .and_then(|snapshot| snapshot.value::<OwnerData>())
                            .and_then(|data| data.get::<u64>())
                            .copied()
                            .ok_or_else(|| {
                                SystemHandlerError::Rejected("missing typed owner state".into())
                            })?;
                        Ok(OwnerPatch::new(
                            job,
                            OwnerData::new(value + 1),
                            PatchUsage {
                                writes: 1,
                                effects: 0,
                                estimated_bytes: std::mem::size_of::<u64>(),
                            },
                        ))
                    },
                )
                .unwrap();
        }
        let plan = registry.freeze().unwrap();
        assert_eq!(
            plan.system(&systems[0]).unwrap().wave_index(),
            plan.system(&systems[1]).unwrap().wave_index(),
            "independent test systems must share one logical dependency wave"
        );
        state.phase_plan = plan;
        for system in &systems {
            register_u64_state(&mut state.system_runtime, system);
            for x in 0..8 {
                state
                    .system_runtime
                    .insert_owner(
                        system.clone(),
                        OwnerKey::chunk(ChunkKey { x, y: 0, z: 0 }),
                        x as u64,
                    )
                    .unwrap();
            }
        }

        tick_with_inputs(&mut state, TickId::new(1), Instant::now(), vec![], vec![]).unwrap();
        let committed = systems
            .iter()
            .flat_map(|system| {
                (0..8).map(|x| {
                    state
                        .system_runtime
                        .owner_value::<u64>(system, OwnerKey::chunk(ChunkKey { x, y: 0, z: 0 }))
                        .unwrap()
                })
            })
            .collect();
        let distinct_workers = worker_ids.lock().unwrap().len();
        drop(state);
        (committed, distinct_workers)
    }

    let (single_thread, one_worker_count) = run(1);
    let (four_threads, four_worker_count) = run(4);
    assert_eq!(single_thread, four_threads);
    let expected = (0..2)
        .flat_map(|_| (0..8).map(|x| (1, x + 1)))
        .collect::<Vec<_>>();
    assert_eq!(single_thread, expected);
    assert_eq!(one_worker_count, 1);
    assert_eq!(four_worker_count, 4);
}

// --- Registered owner effects ------------------------------------------------
//
// The scenario below is the proof that delivery is optional: a saturating
// counter system emits one wake, the woken owner converges sooner with
// effects live, and the same final state is reached with every effect
// dropped before delivery.

/// Wake-like test payload: carry-nothing `amount` addressed at one owner.
/// The destination still does its own durable work through its normal
/// handler; delivery only schedules it sooner.
#[derive(Clone)]
struct Nudge {
    to: OwnerKey,
    amount: u32,
}

fn chunk_owner(x: i32) -> OwnerKey {
    OwnerKey::chunk(ChunkKey { x, y: 0, z: 0 })
}

struct NudgeHarness {
    _save: TestSave,
    state: crate::server::State,
    system: SystemId,
    owners: [OwnerKey; 3],
    deliveries: DeliveryLog,
}

/// Recorded consumer deliveries: the destination owner plus each kind's
/// payload amounts in stable delivery order.
type DeliveryRecord = (OwnerKey, Vec<u32>);
type DeliveryLog = Arc<Mutex<Vec<DeliveryRecord>>>;

#[allow(clippy::too_many_arguments)]
fn nudge_harness<H>(
    workers: usize,
    max_jobs: usize,
    max_effects_per_tick: usize,
    max_effects_per_job: usize,
    drop_effects: bool,
    seeds: [u64; 3],
    consumer_usage: PatchUsage,
    handler: H,
) -> NudgeHarness
where
    H: SystemHandler,
{
    let deliveries = Arc::new(Mutex::new(Vec::new()));
    let kind = EffectKindId::new("test:nudge").unwrap();
    let mut kinds = EffectKindRegistry::new();
    let delivery_log = Arc::clone(&deliveries);
    kinds
        .register(
            kind,
            1,
            64,
            4,
            |_: &Nudge| 8usize,
            |nudge: &Nudge| {
                if nudge.amount == 0 {
                    Err("zero nudge".to_owned())
                } else {
                    Ok(())
                }
            },
            |nudge: &Nudge| Ok(vec![nudge.to]),
            move |job: &OwnerJob, nudges: &[&Nudge]| {
                delivery_log.lock().unwrap().push((
                    job.owner(),
                    nudges.iter().map(|nudge| nudge.amount).collect(),
                ));
                let sum = nudges.iter().map(|nudge| nudge.amount).sum::<u32>();
                Ok(EffectConsumerOutput::new(sum, consumer_usage))
            },
        )
        .unwrap();
    let save = TestSave::new();
    let mut state = crate::server::server_state(113, save.0.clone()).unwrap();
    state.system_runtime = SystemRuntime::new(workers).unwrap();
    state
        .system_runtime
        .set_drop_registered_effects(drop_effects);
    state.effect_kinds = Arc::new(kinds.freeze());

    let system = SystemId::new("test:nudge_system").unwrap();
    let mut registry = SystemRegistry::new();
    crate::server::builtins::register_builtin_systems(&mut registry).unwrap();
    registry
        .register_handler(
            SystemDescriptor::new(
                system.clone(),
                Phase::Simulation,
                OwnerPartition::Chunk,
                max_jobs,
                max_effects_per_tick,
            )
            .effects_per_job(max_effects_per_job)
            .write(ResourceId::new("test:nudge_state").unwrap()),
            handler,
        )
        .unwrap();
    state.phase_plan = registry.freeze().unwrap();
    register_u64_state(&mut state.system_runtime, &system);
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    for (owner, seed) in owners.iter().zip(seeds) {
        state
            .system_runtime
            .insert_owner(system.clone(), *owner, seed)
            .unwrap();
    }
    NudgeHarness {
        _save: save,
        state,
        system,
        owners,
        deliveries,
    }
}

fn tick_nudge(harness: &mut NudgeHarness, tick: u64) -> std::io::Result<()> {
    tick_with_inputs(
        &mut harness.state,
        TickId::new(tick),
        Instant::now(),
        vec![],
        vec![],
    )
}

#[test]
fn registered_wave_uses_due_index_and_rotates_over_ready_owners() {
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    let runs = Arc::new(Mutex::new(Vec::new()));
    let mut harness = nudge_harness(
        1,
        1,
        8,
        4,
        false,
        [0, 0, 0],
        PatchUsage::default(),
        saturating_emitter(owners[0], owners[2], Arc::clone(&runs), 100),
    );
    let scheduled = harness
        .state
        .system_runtime
        .prepare_owner_wave(
            &harness.system,
            vec![OwnerWrite::new(owners[0], 0, OwnerData::new(0u64)).scheduled(Some(10))],
        )
        .unwrap();
    harness
        .state
        .system_runtime
        .apply_replayed_owner_changes(scheduled.changes())
        .unwrap();
    for tick in 1..=4 {
        tick_nudge(&mut harness, tick).unwrap();
    }
    assert_eq!(
        *runs.lock().unwrap(),
        vec![owners[1], owners[2], owners[1], owners[2]]
    );
    for tick in 5..=12 {
        tick_nudge(&mut harness, tick).unwrap();
    }
    assert!(
        runs.lock().unwrap().contains(&owners[0]),
        "due owner must rejoin the rotation"
    );
}

#[test]
fn recurring_wakes_leave_bounded_turns_for_ordinary_owners() {
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    let runs = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&runs);
    let mut harness = nudge_harness(
        1,
        1,
        8,
        4,
        false,
        [0, 0, 0],
        PatchUsage::default(),
        move |job: &OwnerJob| {
            recorded.lock().unwrap().push(job.owner());
            let value = *job
                .snapshot(job.owner())
                .and_then(|snapshot| snapshot.value::<OwnerData>())
                .and_then(|data| data.get::<u64>())
                .ok_or_else(|| SystemHandlerError::Rejected("missing owner".into()))?;
            Ok(OwnerPatch::new(
                job,
                OwnerEffectPatch::new(
                    OwnerData::new(value + 1),
                    vec![EmittedOwnerEffect::new(
                        EffectKindId::new("test:nudge").unwrap(),
                        Nudge {
                            to: owners[2],
                            amount: 1,
                        },
                    )],
                ),
                PatchUsage {
                    writes: 1,
                    effects: 1,
                    estimated_bytes: 8,
                },
            ))
        },
    );
    for tick in 1..=12 {
        tick_nudge(&mut harness, tick).unwrap();
    }
    let served = runs.lock().unwrap();
    assert!(served.contains(&owners[0]) && served.contains(&owners[1]));
    assert!(served.contains(&owners[2]));
    assert_eq!(served.len(), 12);
}

fn nudge_values(harness: &NudgeHarness) -> [u64; 3] {
    harness.owners.map(|owner| {
        harness
            .state
            .system_runtime
            .owner_value::<u64>(&harness.system, owner)
            .unwrap()
            .1
    })
}

fn nudge_revisions(harness: &NudgeHarness) -> [u64; 3] {
    harness.owners.map(|owner| {
        harness
            .state
            .system_runtime
            .owner_value::<u64>(&harness.system, owner)
            .unwrap()
            .0
    })
}

/// Saturating counter. Increments its owner toward `cap`; on its first run
/// `source` emits one nudge at `target`. Every run returns an emission patch
/// so declared usage always matches the emitted count.
fn saturating_emitter(
    source: OwnerKey,
    target: OwnerKey,
    runs: Arc<Mutex<Vec<OwnerKey>>>,
    cap: u64,
) -> impl Fn(&OwnerJob) -> Result<OwnerPatch, SystemHandlerError> + Send + Sync {
    let kind = EffectKindId::new("test:nudge").unwrap();
    move |job: &OwnerJob| {
        runs.lock().unwrap().push(job.owner());
        let value = job
            .snapshot(job.owner())
            .and_then(|snapshot| snapshot.value::<OwnerData>())
            .and_then(|data| data.get::<u64>())
            .copied()
            .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
        let updated = value.saturating_add(1).min(cap);
        let mut emissions = Vec::new();
        if job.owner() == source && value == 0 {
            emissions.push(EmittedOwnerEffect::new(
                kind.clone(),
                Nudge {
                    to: target,
                    amount: 1,
                },
            ));
        }
        let usage = PatchUsage {
            writes: 1,
            effects: emissions.len(),
            estimated_bytes: 8,
        };
        Ok(OwnerPatch::new(
            job,
            OwnerEffectPatch::new(OwnerData::new(updated), emissions),
            usage,
        ))
    }
}

#[test]
fn owner_effects_are_optional_for_convergence() {
    fn converge(drop_effects: bool) -> (Vec<[u64; 3]>, u64, usize) {
        let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
        let runs = Arc::new(Mutex::new(Vec::new()));
        let mut harness = nudge_harness(
            2,
            1,
            8,
            4,
            drop_effects,
            [0, 3, 0],
            PatchUsage::default(),
            saturating_emitter(owners[0], owners[2], Arc::clone(&runs), 3),
        );
        let mut history = Vec::new();
        let mut target_saturated_at = 0;
        for tick in 1..=12u64 {
            tick_nudge(&mut harness, tick).unwrap();
            let values = nudge_values(&harness);
            if target_saturated_at == 0 && values[2] == 3 {
                target_saturated_at = tick;
            }
            history.push(values);
        }
        let deliveries = harness.deliveries.lock().unwrap().len();
        (history, target_saturated_at, deliveries)
    }

    let (live_history, live_saturated, live_deliveries) = converge(false);
    let (drop_history, drop_saturated, drop_deliveries) = converge(true);
    // Same final state with every effect dropped: a lost effect costs
    // latency, never state.
    assert_eq!(live_history.last(), Some(&[3, 3, 3]));
    assert_eq!(live_history.last(), drop_history.last());
    // ... but the woken owner gets there sooner with delivery live ...
    assert!(
        live_saturated < drop_saturated,
        "live saturated at {live_saturated}, dropped at {drop_saturated}"
    );
    // ... because its consumer actually ran exactly once, ...
    assert_eq!(live_deliveries, 1);
    // ... while dropping delivers nothing and still commits producers.
    assert_eq!(drop_deliveries, 0);
    assert_eq!(drop_history[0], [1, 3, 0]);
    // Intermediate states may diverge; only the final state must match.
    assert_ne!(live_history[7], drop_history[7]);
}

#[test]
fn owner_effects_never_cascade_within_a_tick() {
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    let runs = Arc::new(Mutex::new(Vec::new()));
    let mut harness = nudge_harness(
        2,
        1,
        8,
        4,
        false,
        [0, 3, 0],
        PatchUsage::default(),
        saturating_emitter(owners[0], owners[2], Arc::clone(&runs), 3),
    );
    tick_nudge(&mut harness, 1).unwrap();
    // The producer ran and its consumer ran at the barrier, but the
    // destination handler has not run: its work starts next tick.
    assert_eq!(std::mem::take(&mut *runs.lock().unwrap()), vec![owners[0]]);
    assert_eq!(
        *harness.deliveries.lock().unwrap(),
        vec![(owners[2], vec![1])]
    );
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 1);
    assert_eq!(nudge_values(&harness), [1, 3, 0]);

    tick_nudge(&mut harness, 2).unwrap();
    assert_eq!(std::mem::take(&mut *runs.lock().unwrap()), vec![owners[2]]);
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 0);
    assert_eq!(nudge_values(&harness), [1, 3, 1]);
}

#[test]
fn owner_effect_sets_are_deterministic() {
    fn scenario(workers: usize) -> (Vec<[u64; 3]>, Vec<DeliveryRecord>) {
        let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
        let runs = Arc::new(Mutex::new(Vec::new()));
        let kind = EffectKindId::new("test:nudge").unwrap();
        let ring = owners;
        let mut harness = nudge_harness(
            workers,
            3,
            32,
            8,
            false,
            [0, 0, 0],
            PatchUsage::default(),
            move |job: &OwnerJob| {
                runs.lock().unwrap().push(job.owner());
                let value = job
                    .snapshot(job.owner())
                    .and_then(|snapshot| snapshot.value::<OwnerData>())
                    .and_then(|data| data.get::<u64>())
                    .copied()
                    .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
                let position = ring.iter().position(|owner| *owner == job.owner()).unwrap();
                let emissions = vec![EmittedOwnerEffect::new(
                    kind.clone(),
                    Nudge {
                        to: ring[(position + 1) % ring.len()],
                        amount: 1,
                    },
                )];
                Ok(OwnerPatch::new(
                    job,
                    OwnerEffectPatch::new(
                        OwnerData::new(value.saturating_add(1).min(100)),
                        emissions,
                    ),
                    PatchUsage {
                        writes: 1,
                        effects: 1,
                        estimated_bytes: 8,
                    },
                ))
            },
        );
        for tick in 1..=4u64 {
            tick_nudge(&mut harness, tick).unwrap();
        }
        let log = std::mem::take(&mut *harness.deliveries.lock().unwrap());
        (vec![nudge_values(&harness)], log)
    }

    // Repeated runs over identical inputs produce identical effect sets.
    assert_eq!(scenario(2), scenario(2));
    // Producer identity is stable across worker scheduling orders.
    assert_eq!(scenario(1), scenario(4));
}

#[test]
fn overflowing_owner_effects_defer_the_whole_wave() {
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    let runs = Arc::new(Mutex::new(Vec::new()));
    let kind = EffectKindId::new("test:nudge").unwrap();
    let mut harness = nudge_harness(
        2,
        1,
        8,
        2,
        false,
        [0, 0, 0],
        PatchUsage::default(),
        move |job: &OwnerJob| {
            runs.lock().unwrap().push(job.owner());
            let emissions = vec![
                EmittedOwnerEffect::new(
                    kind.clone(),
                    Nudge {
                        to: owners[1],
                        amount: 1,
                    },
                ),
                EmittedOwnerEffect::new(
                    kind.clone(),
                    Nudge {
                        to: owners[2],
                        amount: 1,
                    },
                ),
                EmittedOwnerEffect::new(
                    kind.clone(),
                    Nudge {
                        to: owners[0],
                        amount: 1,
                    },
                ),
            ];
            Ok(OwnerPatch::new(
                job,
                OwnerEffectPatch::new(OwnerData::new(1u64), emissions),
                PatchUsage {
                    writes: 1,
                    effects: 3,
                    estimated_bytes: 8,
                },
            ))
        },
    );
    let before = nudge_revisions(&harness);
    let error = tick_nudge(&mut harness, 1).unwrap_err();
    // Over-bound work defers; it never stops the coordinator.
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_ne!(error.kind(), ErrorKind::InvalidData);
    assert!(!harness.state.durability.failed);
    // Nothing committed: the truncated producer output was rejected whole.
    assert_eq!(nudge_revisions(&harness), before);
    assert_eq!(nudge_values(&harness), [0, 0, 0]);
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 0);
    assert!(harness.deliveries.lock().unwrap().is_empty());
}

#[test]
fn unknown_effect_kinds_are_rejected() {
    let unknown = EffectKindId::new("test:unregistered").unwrap();
    let mut harness = nudge_harness(
        1,
        1,
        8,
        4,
        false,
        [0, 0, 0],
        PatchUsage::default(),
        move |job: &OwnerJob| {
            let emissions = vec![EmittedOwnerEffect::new(unknown.clone(), 7u32)];
            Ok(OwnerPatch::new(
                job,
                OwnerEffectPatch::new(OwnerData::new(1u64), emissions),
                PatchUsage {
                    writes: 1,
                    effects: 1,
                    estimated_bytes: 8,
                },
            ))
        },
    );
    let before = nudge_revisions(&harness);
    let system = harness
        .state
        .phase_plan
        .system(&harness.system)
        .unwrap()
        .clone();
    let kinds = Arc::clone(&harness.state.effect_kinds);
    let error = harness
        .state
        .system_runtime
        .stage_registered_wave(
            &system,
            TickId::new(1),
            0,
            &kinds,
            &mut harness.state.durability,
            &[],
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_ne!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(nudge_revisions(&harness), before);
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 0);
}

#[test]
fn dishonest_effect_accounting_is_rejected() {
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    let kind = EffectKindId::new("test:nudge").unwrap();
    let mut harness = nudge_harness(
        1,
        1,
        8,
        4,
        false,
        [0, 0, 0],
        PatchUsage::default(),
        move |job: &OwnerJob| {
            let emissions = vec![EmittedOwnerEffect::new(
                kind.clone(),
                Nudge {
                    to: owners[1],
                    amount: 1,
                },
            )];
            Ok(OwnerPatch::new(
                job,
                OwnerEffectPatch::new(OwnerData::new(1u64), emissions),
                PatchUsage {
                    writes: 1,
                    effects: 0,
                    estimated_bytes: 8,
                },
            ))
        },
    );
    let before = nudge_revisions(&harness);
    let system = harness
        .state
        .phase_plan
        .system(&harness.system)
        .unwrap()
        .clone();
    let kinds = Arc::clone(&harness.state.effect_kinds);
    let error = harness
        .state
        .system_runtime
        .stage_registered_wave(
            &system,
            TickId::new(1),
            0,
            &kinds,
            &mut harness.state.durability,
            &[],
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_eq!(nudge_revisions(&harness), before);
}

#[test]
fn effect_consumers_cannot_gain_a_write_path() {
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    let kind = EffectKindId::new("test:nudge").unwrap();
    let mut harness = nudge_harness(
        1,
        1,
        8,
        4,
        false,
        [0, 0, 0],
        PatchUsage {
            writes: 1,
            effects: 0,
            estimated_bytes: 4,
        },
        move |job: &OwnerJob| {
            let emissions = vec![EmittedOwnerEffect::new(
                kind.clone(),
                Nudge {
                    to: owners[1],
                    amount: 1,
                },
            )];
            Ok(OwnerPatch::new(
                job,
                OwnerEffectPatch::new(OwnerData::new(1u64), emissions),
                PatchUsage {
                    writes: 1,
                    effects: 1,
                    estimated_bytes: 8,
                },
            ))
        },
    );
    let before = nudge_revisions(&harness);
    let system = harness
        .state
        .phase_plan
        .system(&harness.system)
        .unwrap()
        .clone();
    let kinds = Arc::clone(&harness.state.effect_kinds);
    let error = harness
        .state
        .system_runtime
        .stage_registered_wave(
            &system,
            TickId::new(1),
            0,
            &kinds,
            &mut harness.state.durability,
            &[],
        )
        .unwrap_err();
    // The consumer declared a write, so the whole wave defers before the
    // producer's replacement can commit: no consumer write path exists.
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_eq!(nudge_revisions(&harness), before);
    assert_eq!(nudge_values(&harness), [0, 0, 0]);
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 0);
}

#[test]
fn replayed_cursor_changes_advance_the_rotation_cursor() {
    use crate::server::journal::Change;
    use crate::server::registry::SystemId;
    use crate::server::runtime::owner_codec::{
        OWNER_CURSOR_DOMAIN, decode_owner_cursor_key, encode_cursor_value, owner_cursor_key,
    };
    use crate::server::runtime::owner_wake::{encode_wake_value, owner_wake_key};

    let system = SystemId::new("test:cursor_replay").unwrap();
    let first = chunk_owner(1);
    let second = chunk_owner(2);
    // A receipted cursor change converges the in-memory rotation cursor, and
    // a receipted wake flag for an unloaded owner is held durably. Both must
    // survive the publication path instead of being dropped.
    let cursor = Change::new(
        owner_cursor_key(&system),
        Vec::new(),
        encode_cursor_value(first),
    );
    assert_eq!(cursor.key.domain, OWNER_CURSOR_DOMAIN);
    assert!(decode_owner_cursor_key(&cursor.key).is_some());
    let wake = Change::new(
        owner_wake_key(&system, second),
        Vec::new(),
        encode_wake_value(7),
    );
    let mut runtime = SystemRuntime::new(1).unwrap();
    register_u64_state(&mut runtime, &system);
    runtime
        .apply_replayed_owner_changes(&[cursor, wake])
        .unwrap();
    // Cursor advanced; wake held for the unloaded destination.
    let replayed = Change::new(
        owner_cursor_key(&system),
        encode_cursor_value(first),
        encode_cursor_value(second),
    );
    runtime.apply_replayed_owner_changes(&[replayed]).unwrap();
    assert_eq!(runtime.durable_wake_count(), 1);
    // A stale cursor before-value is corruption, not a skip.
    let stale = Change::new(
        owner_cursor_key(&system),
        Vec::new(),
        encode_cursor_value(first),
    );
    let error = runtime.apply_replayed_owner_changes(&[stale]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

#[test]
fn replayed_wake_flags_land_in_the_durable_set() {
    use crate::server::journal::Change;
    use crate::server::runtime::owner_wake::{encode_wake_value, owner_wake_key};

    let mut runtime = SystemRuntime::new(1).unwrap();
    assert_eq!(runtime.durable_wake_count(), 0);
    let system = SystemId::new("test:late_owner").unwrap();
    let owner = chunk_owner(9);
    // A receipted set-flag for a destination with no live cell is held
    // durably instead of being skipped: it is due, not lost.
    let set = Change::new(
        owner_wake_key(&system, owner),
        Vec::new(),
        encode_wake_value(4),
    );
    runtime.apply_replayed_owner_changes(&[set]).unwrap();
    assert_eq!(runtime.durable_wake_count(), 1);
    assert_eq!(runtime.pending_wake_count(), 0);
    // A receipted clear removes the flag; replay converges.
    let clear = Change::new(
        owner_wake_key(&system, owner),
        encode_wake_value(4),
        Vec::new(),
    );
    runtime.apply_replayed_owner_changes(&[clear]).unwrap();
    assert_eq!(runtime.durable_wake_count(), 0);
}

#[test]
fn wakes_to_unloaded_owners_stage_durably_with_the_producer_wave() {
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    let missing = chunk_owner(99);
    let runs = Arc::new(Mutex::new(Vec::new()));
    // `missing` is never inserted: no live cell exists anywhere.
    let mut harness = nudge_harness(
        2,
        1,
        8,
        4,
        false,
        [0, 3, 0],
        PatchUsage::default(),
        saturating_emitter(owners[0], missing, Arc::clone(&runs), 3),
    );
    tick_nudge(&mut harness, 1).unwrap();
    // The producer committed its own work ...
    assert_eq!(nudge_values(&harness), [1, 3, 0]);
    // ... and the wake to the unloaded owner is held durably, not skipped
    // and not staged as a live wake.
    assert_eq!(harness.state.system_runtime.durable_wake_count(), 1);
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 0);
    // No consumer ran: there is no live destination to consume.
    assert!(harness.deliveries.lock().unwrap().is_empty());
}

#[test]
fn deferred_producer_waves_restage_their_durable_wakes() {
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    let missing = chunk_owner(99);
    let runs = Arc::new(Mutex::new(Vec::new()));
    let mut harness = nudge_harness(
        2,
        1,
        8,
        4,
        false,
        [0, 3, 0],
        PatchUsage::default(),
        saturating_emitter(owners[0], missing, Arc::clone(&runs), 3),
    );
    // A requested rotation defers the wave before its receipt: nothing
    // commits and no durable flag is left half-staged.
    harness.state.durability.rotation_requested = true;
    let error = tick_nudge(&mut harness, 1).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_eq!(nudge_values(&harness), [0, 3, 0]);
    assert_eq!(harness.state.system_runtime.durable_wake_count(), 0);
    // The retry re-stages the flag with the retried record.
    harness.state.durability.rotation_requested = false;
    tick_nudge(&mut harness, 2).unwrap();
    assert_eq!(nudge_values(&harness), [1, 3, 0]);
    assert_eq!(harness.state.system_runtime.durable_wake_count(), 1);
}

// --- Parallel durable commits ------------------------------------------------
//
// The wave below is staged without blocking, polled without blocking, and
// applied only after its receipt. These tests prove the split contract:
// progress while a fsync is in flight, no unconfirmed visibility, disjoint
// waves never blocking each other, and overlapping waves serializing with
// exactly one winner.

use crate::server::durable::complete_barrier;
use crate::server::runtime::owner_durable::OwnerWrite;
use crate::server::runtime::systems::StagedOwnerCommit;

/// Builds live (non-WAL-seeded) owner state over a real journal: enough to
/// stage and poll real receipts through the real writer.
fn staged_harness() -> (TestSave, crate::server::State, SystemId, [OwnerKey; 2]) {
    let save = TestSave::new();
    let mut state = crate::server::server_state(115, save.0.clone()).unwrap();
    state.system_runtime = SystemRuntime::new(2).unwrap();
    let system = SystemId::new("test:staged_owner").unwrap();
    register_u64_state(&mut state.system_runtime, &system);
    let owners = [chunk_owner(0), chunk_owner(1)];
    for (owner, seed) in owners.iter().zip([10u64, 20]) {
        state
            .system_runtime
            .insert_owner(system.clone(), *owner, seed)
            .unwrap();
    }
    (save, state, system, owners)
}

/// The production barrier waits on the explicit admission frontier, not a
/// sleep/retry loop or a feature-specific receipt poll.
fn poll_to_applied(state: &mut crate::server::State, staged: StagedOwnerCommit) -> usize {
    complete_barrier(state, staged.barrier)
        .unwrap()
        .owner_writes
}

#[test]
fn staged_owner_waves_apply_only_after_their_receipt() {
    let (_save, mut state, system, owners) = staged_harness();
    let (revision, _) = state
        .system_runtime
        .owner_value::<u64>(&system, owners[0])
        .unwrap();
    let staged = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_test_wave(
                &system,
                vec![OwnerWrite::new(owners[0], revision, OwnerData::new(41u64))],
                TickId::new(1),
                durability,
            )
            .unwrap()
    };
    // Staging reserves the key but applies nothing: no unconfirmed work is
    // ever visible.
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[0]),
        Some((revision, 10))
    );
    assert_eq!(poll_to_applied(&mut state, staged), 1);
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[0]),
        Some((revision + 1, 41))
    );
    assert!(!state.durability.failed);
}

#[test]
fn disjoint_staged_waves_do_not_block_each_other() {
    let (_save, mut state, system, owners) = staged_harness();
    // Stage the second wave while the first fsync is still in flight: the
    // coordinator makes progress because disjoint key sets never block each
    // other — by construction, not by timing.
    let first = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_test_wave(
                &system,
                vec![OwnerWrite::new(owners[0], 0, OwnerData::new(41u64))],
                TickId::new(1),
                durability,
            )
            .unwrap()
    };
    let second = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_test_wave(
                &system,
                vec![OwnerWrite::new(owners[1], 0, OwnerData::new(42u64))],
                TickId::new(1),
                durability,
            )
            .unwrap()
    };
    // Neither record is confirmed yet, so neither is visible.
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[0]),
        Some((0, 10))
    );
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[1]),
        Some((0, 20))
    );
    assert_eq!(poll_to_applied(&mut state, first), 1);
    assert_eq!(poll_to_applied(&mut state, second), 1);
    // Both applied whole: no lost update, nothing created or destroyed.
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[0]),
        Some((1, 41))
    );
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[1]),
        Some((1, 42))
    );
    assert!(!state.durability.failed);
}

#[test]
fn overlapping_staged_waves_serialize_one_wins_the_other_retries() {
    let (_save, mut state, system, owners) = staged_harness();
    let staged = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_test_wave(
                &system,
                vec![OwnerWrite::new(owners[0], 0, OwnerData::new(41u64))],
                TickId::new(1),
                durability,
            )
            .unwrap()
    };
    // The same key staged twice in flight serializes: the second wave defers
    // with `WouldBlock` — exactly one wins — and never with `InvalidData`.
    let conflict = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime.stage_test_wave(
            &system,
            vec![OwnerWrite::new(owners[0], 0, OwnerData::new(42u64))],
            TickId::new(1),
            durability,
        )
    };
    let error = conflict.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_ne!(error.kind(), ErrorKind::InvalidData);
    assert!(!state.durability.failed);
    assert_eq!(poll_to_applied(&mut state, staged), 1);
    // The loser retries against the winner's receipted revision and commits:
    // no lost update, no duplicated state.
    let retry = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_test_wave(
                &system,
                vec![OwnerWrite::new(owners[0], 1, OwnerData::new(42u64))],
                TickId::new(2),
                durability,
            )
            .unwrap()
    };
    assert_eq!(poll_to_applied(&mut state, retry), 1);
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[0]),
        Some((2, 42))
    );
    assert!(!state.durability.failed);
}

#[test]
fn parallel_and_serial_owner_waves_reach_identical_state_and_receipts() {
    // The workload mixes overlapping revisions (the ring emitter reads its
    // own owner every tick, chaining on the last receipt) with disjoint
    // owners committing in one wave. Same inputs must produce the same
    // plans, the same commit results, and the same final state regardless
    // of worker count or scheduling order.
    fn run(workers: usize) -> (Vec<(u64, u64)>, u64, Vec<DeliveryRecord>) {
        let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
        let runs = Arc::new(Mutex::new(Vec::new()));
        let kind = EffectKindId::new("test:nudge").unwrap();
        let ring = owners;
        let mut harness = nudge_harness(
            workers,
            3,
            32,
            8,
            false,
            [0, 0, 0],
            PatchUsage::default(),
            move |job: &OwnerJob| {
                runs.lock().unwrap().push(job.owner());
                let value = job
                    .snapshot(job.owner())
                    .and_then(|snapshot| snapshot.value::<OwnerData>())
                    .and_then(|data| data.get::<u64>())
                    .copied()
                    .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
                let position = ring.iter().position(|owner| *owner == job.owner()).unwrap();
                let emissions = vec![EmittedOwnerEffect::new(
                    kind.clone(),
                    Nudge {
                        to: ring[(position + 1) % ring.len()],
                        amount: 1,
                    },
                )];
                Ok(OwnerPatch::new(
                    job,
                    OwnerEffectPatch::new(
                        OwnerData::new(value.saturating_add(1).min(100)),
                        emissions,
                    ),
                    PatchUsage {
                        writes: 1,
                        effects: 1,
                        estimated_bytes: 8,
                    },
                ))
            },
        );
        for tick in 1..=6u64 {
            tick_nudge(&mut harness, tick).unwrap();
        }
        let finals = owners
            .map(|owner| {
                harness
                    .state
                    .system_runtime
                    .owner_value::<u64>(&harness.system, owner)
                    .unwrap()
            })
            .to_vec();
        // Identical commit receipts: the same number of WAL records in the
        // same order.
        let sequence = harness.state.durability.writer.sequence();
        let log = std::mem::take(&mut *harness.deliveries.lock().unwrap());
        (finals, sequence, log)
    }

    let serial = run(1);
    let parallel = run(4);
    assert_eq!(serial.0, parallel.0);
    assert_eq!(serial.1, parallel.1);
    assert_eq!(serial.2, parallel.2);
}

// --- Live multi-wave dispatch ------------------------------------------------
//
// The coordinator loop stages every registered wave in a phase before polling
// any receipt, arbitrating each candidate against the already-staged key
// sets. These tests drive that production path (`stage_registered_wave` /
// the shared durable barrier — the same path `tick_with_inputs` calls):
// progress while a receipt is deliberately left unpolled, no unconfirmed
// visibility, deterministic arbitration, and identical multi-wave commits at
// any worker count.

/// Two incrementing owner systems sharing one Simulation phase: every live
/// tick stages two waves, so each tick has two waves in flight at once.
struct TwinHarness {
    _save: TestSave,
    state: crate::server::State,
    systems: [SystemId; 2],
    owners: [[OwnerKey; 2]; 2],
}

fn twin_harness(workers: usize) -> TwinHarness {
    let save = TestSave::new();
    let mut state = crate::server::server_state(117, save.0.clone()).unwrap();
    state.system_runtime = SystemRuntime::new(workers).unwrap();
    let mut registry = SystemRegistry::new();
    crate::server::builtins::register_builtin_systems(&mut registry).unwrap();
    let systems = [
        SystemId::new("test:twin_a").unwrap(),
        SystemId::new("test:twin_b").unwrap(),
    ];
    for system in &systems {
        let resource =
            ResourceId::new(format!("test:{}_state", system.as_str().replace(':', "_"))).unwrap();
        registry
            .register_handler(
                SystemDescriptor::new(
                    system.clone(),
                    Phase::Simulation,
                    OwnerPartition::Chunk,
                    2,
                    0,
                )
                .write(resource),
                |job: &OwnerJob| {
                    let value = job
                        .snapshot(job.owner())
                        .and_then(|snapshot| snapshot.value::<OwnerData>())
                        .and_then(|data| data.get::<u64>())
                        .copied()
                        .ok_or_else(|| {
                            SystemHandlerError::Rejected("missing owner state".into())
                        })?;
                    Ok(OwnerPatch::new(
                        job,
                        OwnerData::new(value + 1),
                        PatchUsage {
                            writes: 1,
                            effects: 0,
                            estimated_bytes: 8,
                        },
                    ))
                },
            )
            .unwrap();
    }
    state.phase_plan = registry.freeze().unwrap();
    let owners = [
        [chunk_owner(0), chunk_owner(1)],
        [chunk_owner(10), chunk_owner(11)],
    ];
    let seeds = [[10u64, 20], [30, 40]];
    for ((system, owners), seeds) in systems.iter().zip(owners.iter()).zip(seeds.iter()) {
        register_u64_state(&mut state.system_runtime, system);
        for (owner, seed) in owners.iter().zip(seeds.iter()) {
            state
                .system_runtime
                .insert_owner(system.clone(), *owner, *seed)
                .unwrap();
        }
    }
    TwinHarness {
        _save: save,
        state,
        systems,
        owners,
    }
}

fn twin_values(harness: &TwinHarness) -> Vec<u64> {
    harness
        .systems
        .iter()
        .zip(harness.owners.iter())
        .flat_map(|(system, owners)| {
            owners.iter().map(|owner| {
                harness
                    .state
                    .system_runtime
                    .owner_value::<u64>(system, *owner)
                    .unwrap()
                    .1
            })
        })
        .collect()
}

#[test]
fn live_coordinator_stages_disjoint_waves_before_polling_any_receipt() {
    let mut harness = twin_harness(2);
    let kinds = Arc::clone(&harness.state.effect_kinds);
    let first = harness
        .state
        .phase_plan
        .system(&harness.systems[0])
        .unwrap()
        .clone();
    let second = harness
        .state
        .phase_plan
        .system(&harness.systems[1])
        .unwrap()
        .clone();
    // Stage the first wave through the production entry the coordinator loop
    // calls, then deliberately leave its receipt unpolled: from the
    // coordinator's side a slow journal looks exactly like this.
    let staged_first = {
        let (runtime, durability) = (
            &mut harness.state.system_runtime,
            &mut harness.state.durability,
        );
        runtime
            .stage_registered_wave(&first, TickId::new(1), 0, &kinds, durability, &[])
            .unwrap()
            .expect("first twin wave stages")
    };
    // Progress while the first fsync is still unobserved: the second system
    // prepares and stages against the first wave's in-flight key set. The key
    // sets are disjoint, so arbitration commits both — by construction, not
    // by timing — and the second stage succeeds while the first receipt is
    // still pending.
    let staged_second = {
        let (runtime, durability) = (
            &mut harness.state.system_runtime,
            &mut harness.state.durability,
        );
        runtime
            .stage_registered_wave(
                &second,
                TickId::new(1),
                1,
                &kinds,
                durability,
                &[staged_first.keys().to_vec()],
            )
            .unwrap()
            .expect("disjoint twin wave stages while the first receipt is in flight")
    };
    // Nothing unconfirmed is ever visible: both waves are staged, neither has
    // applied.
    assert_eq!(twin_values(&harness), vec![10, 20, 30, 40]);
    // Both receipts drain through the single shared apply gate, whole.
    let applied = complete_barrier(&mut harness.state, staged_second.barrier()).unwrap();
    assert_eq!(applied.commits, 2);
    assert_eq!(twin_values(&harness), vec![11, 21, 31, 41]);
    assert!(!harness.state.durability.failed);
}

#[test]
fn multi_wave_live_commits_match_at_one_and_four_workers() {
    // Two waves in flight every tick: same inputs must produce the same final
    // state and the same receipt order regardless of worker count.
    fn run(workers: usize) -> (Vec<u64>, u64) {
        let mut harness = twin_harness(workers);
        for tick in 1..=4u64 {
            tick_with_inputs(
                &mut harness.state,
                TickId::new(tick),
                Instant::now(),
                vec![],
                vec![],
            )
            .unwrap();
        }
        let sequence = harness.state.durability.writer.sequence();
        (twin_values(&harness), sequence)
    }

    let (single_values, single_sequence) = run(1);
    assert_eq!(single_values, vec![14, 24, 34, 44]);
    assert_eq!(run(4), (single_values, single_sequence));
}

#[test]
fn overlapping_production_waves_retry_then_commit_without_loss() {
    let mut harness = twin_harness(2);
    let kinds = Arc::clone(&harness.state.effect_kinds);
    let system = harness
        .state
        .phase_plan
        .system(&harness.systems[0])
        .unwrap()
        .clone();
    let wave = {
        let (runtime, durability) = (
            &mut harness.state.system_runtime,
            &mut harness.state.durability,
        );
        runtime
            .stage_registered_wave(&system, TickId::new(1), 0, &kinds, durability, &[])
            .unwrap()
            .expect("first wave stages")
    };
    // Same system restaged at the same revision touches the same keys: the
    // second wave shares every key with the in-flight winner, so arbitration
    // defers it with `WouldBlock` — exactly one wins — and never with
    // `InvalidData`, which capacity conditions must never reach.
    let conflict = {
        let (runtime, durability) = (
            &mut harness.state.system_runtime,
            &mut harness.state.durability,
        );
        runtime.stage_registered_wave(
            &system,
            TickId::new(1),
            1,
            &kinds,
            durability,
            &[wave.keys().to_vec()],
        )
    };
    let error = conflict.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_ne!(error.kind(), ErrorKind::InvalidData);
    assert!(!harness.state.durability.failed);
    // Nothing unconfirmed is visible while the winner is still in flight.
    assert_eq!(twin_values(&harness), vec![10, 20, 30, 40]);
    // The winner applies whole; the loser retries against the receipted
    // revision and commits: no lost update, nothing created or destroyed.
    let applied = complete_barrier(&mut harness.state, wave.barrier()).unwrap();
    assert_eq!(applied.commits, 1);
    assert_eq!(twin_values(&harness), vec![11, 21, 30, 40]);
    let retry = {
        let (runtime, durability) = (
            &mut harness.state.system_runtime,
            &mut harness.state.durability,
        );
        runtime
            .stage_registered_wave(&system, TickId::new(2), 0, &kinds, durability, &[])
            .unwrap()
            .expect("retry stages after the winner's receipt")
    };
    let applied = complete_barrier(&mut harness.state, retry.barrier()).unwrap();
    assert_eq!(applied.commits, 1);
    assert_eq!(twin_values(&harness), vec![12, 22, 30, 40]);
    assert!(!harness.state.durability.failed);
}

#[test]
fn arbitrated_retry_withdraws_staged_wake_flags() {
    use std::collections::VecDeque;

    // One owner that always wakes an unloaded destination, with the
    // destination popped from a script so each prepared wave stages a fresh
    // flag: X, then Y (withdrawn on retry), then Z, then Y again. If the
    // retry leaked its staged flag, the final Y would chain onto the leak
    // and never reach the durable set.
    let script = Arc::new(Mutex::new(VecDeque::from([
        chunk_owner(91),
        chunk_owner(92),
        chunk_owner(93),
        chunk_owner(92),
    ])));
    let deliveries: DeliveryLog = Arc::new(Mutex::new(Vec::new()));
    let kind = EffectKindId::new("test:retry_wake").unwrap();
    let mut kinds = EffectKindRegistry::new();
    let delivery_log = Arc::clone(&deliveries);
    kinds
        .register(
            kind.clone(),
            1,
            64,
            4,
            |_: &Nudge| 8usize,
            |nudge: &Nudge| {
                if nudge.amount == 0 {
                    Err("zero nudge".to_owned())
                } else {
                    Ok(())
                }
            },
            |nudge: &Nudge| Ok(vec![nudge.to]),
            move |job: &OwnerJob, nudges: &[&Nudge]| {
                delivery_log.lock().unwrap().push((
                    job.owner(),
                    nudges.iter().map(|nudge| nudge.amount).collect(),
                ));
                let sum = nudges.iter().map(|nudge| nudge.amount).sum::<u32>();
                Ok(EffectConsumerOutput::new(sum, PatchUsage::default()))
            },
        )
        .unwrap();
    let save = TestSave::new();
    let mut state = crate::server::server_state(119, save.0.clone()).unwrap();
    state.system_runtime = SystemRuntime::new(1).unwrap();
    state.effect_kinds = Arc::new(kinds.freeze());
    let system = SystemId::new("test:retry_wake").unwrap();
    let mut registry = SystemRegistry::new();
    crate::server::builtins::register_builtin_systems(&mut registry).unwrap();
    registry
        .register_handler(
            SystemDescriptor::new(
                system.clone(),
                Phase::Simulation,
                OwnerPartition::Chunk,
                1,
                8,
            )
            .effects_per_job(4)
            .write(ResourceId::new("test:retry_wake_state").unwrap()),
            move |job: &OwnerJob| {
                let value = job
                    .snapshot(job.owner())
                    .and_then(|snapshot| snapshot.value::<OwnerData>())
                    .and_then(|data| data.get::<u64>())
                    .copied()
                    .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
                let to = script
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("wake script remains");
                let emissions = vec![EmittedOwnerEffect::new(
                    kind.clone(),
                    Nudge { to, amount: 1 },
                )];
                Ok(OwnerPatch::new(
                    job,
                    OwnerEffectPatch::new(OwnerData::new(value + 1), emissions),
                    PatchUsage {
                        writes: 1,
                        effects: 1,
                        estimated_bytes: 8,
                    },
                ))
            },
        )
        .unwrap();
    state.phase_plan = registry.freeze().unwrap();
    register_u64_state(&mut state.system_runtime, &system);
    let owner = chunk_owner(0);
    state
        .system_runtime
        .insert_owner(system.clone(), owner, 0u64)
        .unwrap();
    let executable = state.phase_plan.system(&system).unwrap().clone();
    let kinds = Arc::clone(&state.effect_kinds);

    let value = |state: &crate::server::State| {
        state
            .system_runtime
            .owner_value::<u64>(&system, owner)
            .unwrap()
            .1
    };
    // Wave 1 stages its flag for X; wave 2 overlaps it and must retry.
    let wave_x = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_registered_wave(&executable, TickId::new(1), 0, &kinds, durability, &[])
            .unwrap()
            .expect("first wave stages")
    };
    let conflict = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime.stage_registered_wave(
            &executable,
            TickId::new(1),
            1,
            &kinds,
            durability,
            &[wave_x.keys().to_vec()],
        )
    };
    assert_eq!(conflict.unwrap_err().kind(), ErrorKind::WouldBlock);
    // The deferral withdrew wave 2's staged flag for Y: nothing is visible
    // and nothing is left half-staged.
    assert_eq!(value(&state), 0);
    assert_eq!(state.system_runtime.durable_wake_count(), 0);
    // Wave 1 applies once with exactly its own flag; wave 3 stages Z fresh.
    assert_eq!(
        complete_barrier(&mut state, wave_x.barrier())
            .unwrap()
            .commits,
        1
    );
    assert_eq!(value(&state), 1);
    assert_eq!(state.system_runtime.durable_wake_count(), 1);
    let wave_z = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_registered_wave(&executable, TickId::new(2), 2, &kinds, durability, &[])
            .unwrap()
            .expect("third wave stages")
    };
    assert_eq!(
        complete_barrier(&mut state, wave_z.barrier())
            .unwrap()
            .commits,
        1
    );
    assert_eq!(value(&state), 2);
    assert_eq!(state.system_runtime.durable_wake_count(), 2);
    // Y re-stages fresh on the retry: a leaked flag would have chained onto
    // the withdrawn set and this record would carry no flag.
    let wave_y = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_registered_wave(&executable, TickId::new(3), 1, &kinds, durability, &[])
            .unwrap()
            .expect("retried Y stages fresh after the withdrawal")
    };
    assert_eq!(
        complete_barrier(&mut state, wave_y.barrier())
            .unwrap()
            .commits,
        1
    );
    // Every wave applied exactly once, in order; every flag landed once; no
    // consumer ever ran because no destination was ever loaded.
    assert_eq!(value(&state), 3);
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&system, owner)
            .unwrap()
            .0,
        3
    );
    assert_eq!(state.system_runtime.durable_wake_count(), 3);
    assert!(deliveries.lock().unwrap().is_empty());
    assert!(!state.durability.failed);
}

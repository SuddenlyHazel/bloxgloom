use super::*;
use crate::server::effects::{EffectConsumerOutput, EffectKindId, EffectKindRegistry};
use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
use crate::server::registry::{
    OwnerPartition, ResourceId, SystemDescriptor, SystemHandler, SystemHandlerError, SystemId,
    SystemRegistry,
};
use crate::server::runtime::owner_effects::{EmittedOwnerEffect, OwnerEffectPatch};
use crate::server::runtime::owner_durable::OwnerSystemConfig;
use crate::server::runtime::owner_codec::{OwnerCodecError, OwnerValueCodec};
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
            OwnerSystemConfig::new(system.clone(), Arc::new(U64OwnerCodec), 1, 8).unwrap(),
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
        .run_registered(&system, TickId::new(1), 0, &kinds, &mut harness.state.durability)
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
        .run_registered(&system, TickId::new(1), 0, &kinds, &mut harness.state.durability)
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
        .run_registered(&system, TickId::new(1), 0, &kinds, &mut harness.state.durability)
        .unwrap_err();
    // The consumer declared a write, so the whole wave defers before the
    // producer's replacement can commit: no consumer write path exists.
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_eq!(nudge_revisions(&harness), before);
    assert_eq!(nudge_values(&harness), [0, 0, 0]);
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 0);
}

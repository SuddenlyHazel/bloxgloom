use super::*;
use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
use crate::server::registry::{
    OwnerPartition, ResourceId, SystemDescriptor, SystemHandlerError, SystemId, SystemRegistry,
};
use crate::server::runtime::systems::SystemRuntime;
use crate::server::simulation::Phase;
use crate::world::ChunkKey;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread::{self, ThreadId};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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

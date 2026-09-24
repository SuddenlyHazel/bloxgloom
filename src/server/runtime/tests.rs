use super::*;
use crate::server::parallel::{OwnerJob, OwnerPatch, PatchUsage};
use crate::server::registry::{
    OwnerPartition, ResourceId, SystemDescriptor, SystemHandlerError, SystemId, SystemRegistry,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static EXTRA_EXECUTIONS: AtomicUsize = AtomicUsize::new(0);

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

fn extra_driver(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    assert_eq!(context.tick.get(), 1);
    EXTRA_EXECUTIONS.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

fn extra_handler(job: &OwnerJob) -> Result<OwnerPatch, SystemHandlerError> {
    Ok(OwnerPatch::new(job, (), PatchUsage::default()))
}

#[test]
fn independently_registered_executable_runs_without_a_runtime_id_match() {
    let save = TestSave::new();
    let mut state = crate::server::server_state(113, save.0.clone()).unwrap();
    let mut registry = SystemRegistry::new();
    crate::server::builtins::register_builtin_systems(&mut registry).unwrap();
    let extra = SystemId::new("test:independent_owner").unwrap();
    registry
        .register_handler_with_driver(
            SystemDescriptor::new(
                extra.clone(),
                Phase::Simulation,
                OwnerPartition::Chunk,
                1,
                0,
            )
            .write(ResourceId::new("test:owner_scratch").unwrap()),
            extra_handler,
            extra_driver,
        )
        .unwrap();
    state.phase_plan = registry.freeze().unwrap();
    let registered = state.phase_plan.system(&extra).unwrap();
    assert!(registered.has_executable_handler());
    assert!(registered.driver().is_some());

    EXTRA_EXECUTIONS.store(0, Ordering::SeqCst);
    tick_with_inputs(&mut state, TickId::new(1), Instant::now(), vec![], vec![]).unwrap();
    assert_eq!(EXTRA_EXECUTIONS.load(Ordering::SeqCst), 1);
    drop(state);
}

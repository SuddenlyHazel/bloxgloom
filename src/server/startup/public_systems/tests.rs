use super::*;
use crate::server::{
    parallel::{BatchId, JobKey, OwnerSnapshot},
    simulation::TickId,
};
use bloxgloom_host_api::RegistrationError;

struct InvalidOutput {
    oversized: bool,
}
impl api::Behavior for InvalidOutput {
    fn validate(&self, _: &[u8]) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn plan(&self, c: &api::Context<'_>) -> Result<api::Plan, RegistrationError> {
        Ok(api::Plan {
            data: vec![0; if self.oversized { 9 } else { 8 }],
            next_tick: if self.oversized { c.tick + 1 } else { c.tick },
        })
    }
}

#[test]
fn owner_adapter_rejects_oversized_output_and_nonadvancing_schedule() {
    let owner = OwnerKey::Chunk(crate::world::ChunkKey { x: 0, y: 0, z: 0 });
    let system = SystemId::new(bloxgloom_lifecycle_fixture::system::KEY).unwrap();
    let job = OwnerJob::new(
        system,
        JobKey::new(
            BatchId::new(TickId::new(50), Phase::Simulation, 0),
            owner,
            0,
            7,
        ),
        vec![OwnerSnapshot::new(
            owner,
            7,
            Arc::new(OwnerData::new(vec![0u8; 8])),
        )],
    )
    .unwrap();
    for oversized in [false, true] {
        let mut d = bloxgloom_lifecycle_fixture::system::definition();
        d.behavior = Arc::new(InvalidOutput { oversized });
        assert!(Adapter(Arc::new(d)).prepare(&job).is_err());
    }
    let snapshot = job.snapshot(owner).unwrap().value::<OwnerData>().unwrap();
    assert_eq!(snapshot.get::<Vec<u8>>().unwrap(), &vec![0; 8]);
}

#[test]
fn owner_declarations_survive_manifest_remap_and_reject_schema_or_dependency_mismatch() {
    let mut catalog = Catalog::builtins();
    catalog
        .register_owner_system(bloxgloom_lifecycle_fixture::system::definition())
        .unwrap();
    let mut manifest = crate::content::ContentManifest::from_catalog(&catalog);
    manifest
        .entries
        .iter_mut()
        .find(|e| e.kind == b'Y')
        .unwrap()
        .id = 77;
    let resolved = manifest.resolve_catalog(&catalog).unwrap();
    assert_eq!(
        crate::content::ContentManifest::from_catalog(&resolved),
        manifest
    );
    assert_eq!(resolved.owner_systems().count(), 1);
    assert!(ServerStartup::new(Arc::new(resolved)).phase_plan().is_ok());
    manifest
        .entries
        .iter_mut()
        .find(|e| e.kind == b'Y')
        .unwrap()
        .schema_fingerprint ^= 1;
    assert!(manifest.resolve_catalog(&catalog).is_err());
    let mut missing = Catalog::builtins();
    let mut d = bloxgloom_lifecycle_fixture::system::definition();
    d.after = vec!["fixture:missing".into()];
    missing.register_owner_system(d).unwrap();
    assert!(ServerStartup::new(Arc::new(missing)).phase_plan().is_err());
}

#[test]
fn owner_world_read_requires_bounded_chunk_ownership_and_changes_the_manifest() {
    let ordinary = bloxgloom_lifecycle_fixture::system::definition();
    let mut world = ordinary.clone();
    world.read_owner_chunk = true;
    assert_ne!(ordinary.fingerprint_bytes(), world.fingerprint_bytes());
    world.partition = api::Partition::Profile;
    assert!(world.validate().is_err());
    world.partition = api::Partition::Chunk;
    world.max_jobs_per_tick = 65;
    assert!(world.validate().is_err());
    world.max_jobs_per_tick = 2;
    assert!(world.validate().is_ok());
}

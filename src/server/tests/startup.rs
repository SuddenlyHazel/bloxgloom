use super::*;

#[test]
fn entity_tick_policy_runs_on_workers_in_durable_coordinator_order() {
    use crate::server::entities::{
        EntityError, EntityPayload, EntitySnapshot, EntitySpawn, EntityTickPlan, EntityTickPolicy,
        EntityView,
    };
    use crate::server::voxel_view::VoxelView;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Condvar, Mutex};

    struct Probe {
        coordinator: std::thread::ThreadId,
        seen: Arc<Mutex<Vec<std::thread::ThreadId>>>,
        reverse: bool,
        gate: Arc<(Mutex<bool>, Condvar)>,
        panic_once: Arc<AtomicBool>,
    }
    impl EntityTickPolicy for Probe {
        fn plan(
            &self,
            snapshot: &EntitySnapshot,
            _tick: u64,
            _catalog: &crate::content::Catalog,
            _view: &VoxelView,
            _neighbours: &EntityView,
        ) -> Result<EntityTickPlan, EntityError> {
            let worker = std::thread::current().id();
            assert_ne!(
                worker, self.coordinator,
                "production tick dispatch ran policy on coordinator"
            );
            let x = match snapshot.location {
                crate::server::entities::EntityLocation::Mobile { position } => position[0],
                _ => unreachable!(),
            };
            if self.reverse {
                let (lock, cv) = &*self.gate;
                if x < 1.0 {
                    let mut done = lock.lock().unwrap();
                    while !*done {
                        done = cv.wait(done).unwrap();
                    }
                } else {
                    *lock.lock().unwrap() = true;
                    cv.notify_all();
                }
            }
            self.seen.lock().unwrap().push(worker);
            if x < 1.0 && self.panic_once.swap(false, Ordering::SeqCst) {
                panic!("test-only entity worker failure");
            }
            Ok(EntityTickPlan {
                payload: None,
                next_tick: snapshot.next_tick.map(|due| due + 1),
                anchor_update: None,
                position: Some([x + 0.125, 80.0, 0.5]),
                block_states: Vec::new(),
                wakes: Vec::new(),
                transfer: None,
            })
        }
    }

    fn run(workers: usize, fail_once: bool) -> (Vec<(u64, u64, [f32; 3])>, usize) {
        let save = TestSave::new("entity-worker-dispatch");
        let entity_type = crate::content::EntityTypeId(70_051);
        let mut catalog = crate::content::Catalog::builtins();
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id: entity_type,
                key: "test:worker_entity".into(),
                schema_version: 1,
                schema_fingerprint: 0x70051,
            })
            .unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut startup = ServerStartup::new(Arc::new(catalog));
        startup.register_entity_type(crate::server::startup::StartupEntityType {
            key: "test:worker_entity".into(),
            ownership: crate::server::entities::EntityOwnership::Mobile,
            tick_policy: crate::server::entities::TickPolicy::EveryTick,
            max_payload_bytes: 1,
            codec: Arc::new(StartupProbeCodec),
            interaction_policy: None,
            tick_planner: Some(Arc::new(Probe {
                coordinator: std::thread::current().id(),
                seen: Arc::clone(&seen),
                reverse: workers > 1 && !fail_once,
                gate: Arc::new((Mutex::new(false), Condvar::new())),
                panic_once: Arc::new(AtomicBool::new(fail_once)),
            })),
        });
        let mut state =
            server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
        state.entity_tick_executor =
            crate::server::parallel::PhaseExecutor::new(workers, 256, 256).unwrap();
        let mut ids = Vec::new();
        for x in [0.5, 1.5] {
            let batch = state
                .entities
                .prepare_spawn(EntitySpawn::Mobile {
                    entity_type,
                    position: [x, 80.0, 0.5],
                    payload: EntityPayload::new(7u8),
                    spawn_tick: 1,
                })
                .unwrap();
            ids.push(batch.entity_id());
            stage_tamper_batch(&mut state, batch, 1);
        }
        for id in &ids {
            state
                .durability
                .queued
                .push_back(crate::server::durable::DurableRequest::EntityTick { id: *id });
        }
        let original = state.entities.snapshot(ids[0]).unwrap();
        crate::server::durable::process_durable_actions(&mut state, TickId::new(2), Instant::now())
            .unwrap();
        if fail_once {
            assert_eq!(
                state.entities.snapshot(ids[0]).unwrap().motion_revision,
                original.motion_revision,
                "failed worker cannot apply a partial entity step"
            );
            assert!(state.durability.queued.iter().any(|request|
                matches!(request, crate::server::durable::DurableRequest::EntityTick { id } if *id == ids[0])));
            crate::server::durable::process_durable_actions(
                &mut state,
                TickId::new(2),
                Instant::now(),
            )
            .unwrap();
        }
        assert!(
            state.durability.pending.is_empty(),
            "motion receipt must apply in the same tick"
        );
        let snapshots = ids
            .iter()
            .map(|id| {
                let snapshot = state.entities.snapshot(*id).unwrap();
                let crate::server::entities::EntityLocation::Mobile { position } =
                    snapshot.location
                else {
                    unreachable!()
                };
                (snapshot.revision, snapshot.motion_revision, position)
            })
            .collect();
        let count = seen.lock().unwrap().len();
        (snapshots, count)
    }
    let single = run(1, false);
    let parallel = run(2, false);
    assert_eq!(single, parallel);
    assert_eq!(parallel.1, 2);
    assert_eq!(parallel.0[0].2[0], 0.625);
    assert_eq!(parallel.0[1].2[0], 1.625);
    let retried = run(2, true);
    assert_eq!(retried.0, parallel.0);
    assert_eq!(retried.1, 3);
}

struct StartupProbeCodec;

impl crate::server::entities::EntityPayloadCodec for StartupProbeCodec {
    fn decode(
        &self,
        bytes: &[u8],
    ) -> Result<crate::server::entities::EntityPayload, crate::server::entities::EntityCodecError>
    {
        let [value] = bytes else {
            return Err(crate::server::entities::EntityCodecError::InvalidData);
        };
        Ok(crate::server::entities::EntityPayload::new(*value))
    }

    fn encode(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        payload
            .downcast_ref::<u8>()
            .copied()
            .map(|value| vec![value])
            .ok_or(crate::server::entities::EntityCodecError::InvalidData)
    }

    fn public_view(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        self.encode(payload)
    }
}

struct StartupOwnerU64Codec;

impl crate::server::runtime::owner_codec::OwnerValueCodec for StartupOwnerU64Codec {
    fn decode(
        &self,
        payload: &[u8],
    ) -> Result<
        crate::server::parallel::OwnerData,
        crate::server::runtime::owner_codec::OwnerCodecError,
    > {
        if payload.len() != 8 {
            return Err(crate::server::runtime::owner_codec::OwnerCodecError::InvalidData);
        }
        Ok(crate::server::parallel::OwnerData::new(u64::from_le_bytes(
            payload.try_into().expect("checked length"),
        )))
    }

    fn encode(
        &self,
        value: &crate::server::parallel::OwnerData,
    ) -> Result<Vec<u8>, crate::server::runtime::owner_codec::OwnerCodecError> {
        value
            .get::<u64>()
            .map(|value| value.to_le_bytes().to_vec())
            .ok_or(crate::server::runtime::owner_codec::OwnerCodecError::InvalidData)
    }
}

/// Registers the little-endian u64 owner codec the startup owner tests use.
/// Production systems register their own codec; a registered system without
/// one is a startup error, never a transient fallback.
fn register_u64_owner_codec(
    startup: &mut ServerStartup,
    system: &crate::server::registry::SystemId,
) {
    startup.register_owner_codec(
        system.clone(),
        crate::server::startup::StartupOwnerCodec {
            codec: Arc::new(StartupOwnerU64Codec),
            codec_version: 1,
            max_bytes: 8,
        },
    );
}

#[test]
fn startup_extension_registers_entity_codec_and_executes_owner_system() {
    use crate::server::entities::{EntityOwnership, EntityPayload, TickPolicy};
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{
        OwnerPartition, ResourceId, SystemDescriptor, SystemHandlerError, SystemId,
    };
    use crate::server::startup::StartupEntityType;

    let mut catalog = crate::content::Catalog::builtins();
    let entity_id = crate::content::EntityTypeId(70_003);
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: entity_id,
            key: "test:probe_entity".into(),
            schema_version: 1,
            schema_fingerprint: 0x5052_4f42_4500_0001,
        })
        .unwrap();
    let mut startup = ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:probe_entity".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 1,
        codec: Arc::new(StartupProbeCodec),
        interaction_policy: None,
        tick_planner: None,
    });

    let system = SystemId::new("test:probe_system").unwrap();
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Chunk,
            2,
            0,
        )
        .write(ResourceId::new("test:probe_state").unwrap()),
        |job: &OwnerJob| {
            let value = job
                .snapshot(job.owner())
                .and_then(|snapshot| snapshot.value::<OwnerData>())
                .and_then(|data| data.get::<u64>())
                .copied()
                .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
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
    );
    register_u64_owner_codec(&mut startup, &system);
    let owner = OwnerKey::chunk(crate::world::ChunkKey { x: 0, y: 0, z: 0 });
    startup.seed_owner(system.clone(), owner, 41u64);

    let save = TestSave::new("startup-extension-live");
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    let descriptor = state.entities.types().descriptor(entity_id).unwrap();
    let payload = EntityPayload::new(7u8);
    assert_eq!(descriptor.encode_payload(&payload).unwrap(), [7]);
    assert_eq!(descriptor.public_view(&payload).unwrap(), [7]);
    assert_eq!(
        descriptor
            .decode_payload(1, &[7])
            .unwrap()
            .downcast_ref::<u8>(),
        Some(&7)
    );
    let spawn = state
        .entities
        .prepare_spawn(crate::server::entities::EntitySpawn::Mobile {
            entity_type: entity_id,
            position: [0.5, 80.0, 0.5],
            payload,
            spawn_tick: 1,
        })
        .unwrap();
    let spawned_id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();
    let checkpoint = crate::server::entities::encode_checkpoint(&state.entities).unwrap();
    let restored = crate::server::entities::decode_checkpoint(
        &checkpoint,
        Arc::new(state.entities.types().clone()),
    )
    .unwrap();
    assert_eq!(restored.public_view(spawned_id).unwrap().payload, [7]);
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owner),
        Some((1, 42))
    );
}

#[test]
fn invalid_startup_owner_seed_does_not_create_a_world() {
    let save = TestSave::new("invalid-startup-owner");
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.seed_owner(
        crate::server::registry::SystemId::new("test:missing_system").unwrap(),
        crate::server::parallel::OwnerKey::chunk(crate::world::ChunkKey { x: 0, y: 0, z: 0 }),
        1u64,
    );
    assert_eq!(std::fs::read_dir(save.path()).unwrap().count(), 0);
    let error = server_state_with_startup(7, save.path().to_path_buf(), 1, startup)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(std::fs::read_dir(save.path()).unwrap().count(), 0);
}

#[test]
fn independent_worlds_keep_their_own_startup_catalogs() {
    let base_save = TestSave::new("base-world-catalog");
    let extended_save = TestSave::new("extended-world-catalog");
    let base = Arc::new(crate::content::Catalog::builtins());
    let mut extended = crate::content::Catalog::builtins();
    let custom_item = crate::content::ItemId(70_002);
    let texture = extended
        .block_type(crate::content::BlockTypeId(3))
        .unwrap()
        .textures
        .top;
    extended
        .register_item(crate::content::ItemDef {
            id: custom_item,
            key: "test:catalog_only".into(),
            name: "Catalog-only token".into(),
            swatch: [0.2, 0.4, 0.7, 1.0],
            texture,
            placeable: None,
            sprite: true,
        })
        .unwrap();
    let extended = Arc::new(extended);

    let base_state = server_state_with_limit_and_catalog(
        7,
        base_save.path().to_path_buf(),
        1,
        Arc::clone(&base),
    )
    .unwrap();
    let extended_state = server_state_with_limit_and_catalog(
        7,
        extended_save.path().to_path_buf(),
        1,
        Arc::clone(&extended),
    )
    .unwrap();
    assert_eq!(base_state.world.catalog().fingerprint(), base.fingerprint());
    assert_eq!(
        extended_state.world.catalog().fingerprint(),
        extended.fingerprint()
    );
    assert_ne!(base.fingerprint(), extended.fingerprint());
    assert!(base_state.world.catalog().item(custom_item).is_none());
    assert!(extended_state.world.catalog().item(custom_item).is_some());

    let mut inventory = Inventory::default();
    assert_eq!(
        inventory.insert_with_catalog(custom_item, 1, extended_state.world.catalog()),
        0
    );
    let encoded =
        InventoryStore::encode_snapshot_with_catalog(&inventory, extended_state.world.catalog())
            .unwrap();
    assert!(
        InventoryStore::decode_snapshot_with_catalog(&encoded, base_state.world.catalog()).is_err()
    );
    assert_eq!(
        InventoryStore::decode_snapshot_with_catalog(&encoded, extended_state.world.catalog())
            .unwrap(),
        inventory
    );
}

#[test]
fn missing_entity_registration_rejects_catalog_before_touching_world_files() {
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: crate::content::EntityTypeId(70_003),
            key: "test:missing_codec".into(),
            schema_version: 1,
            schema_fingerprint: 1,
        })
        .unwrap();
    let catalog = Arc::new(catalog);

    let fresh = TestSave::new("unregistered-entity-fresh");
    assert_eq!(std::fs::read_dir(fresh.path()).unwrap().count(), 0);
    let error =
        server_state_with_limit_and_catalog(7, fresh.path().to_path_buf(), 1, Arc::clone(&catalog))
            .err()
            .unwrap();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(std::fs::read_dir(fresh.path()).unwrap().count(), 0);

    let existing = TestSave::new("unregistered-entity-existing");
    drop(state_for(&existing, 7));
    let before = std::fs::read(existing.path().join("content.map")).unwrap();
    let error = server_state_with_limit_and_catalog(7, existing.path().to_path_buf(), 1, catalog)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(
        std::fs::read(existing.path().join("content.map")).unwrap(),
        before
    );
}

// --- Registered owner-system effects through the real startup path ---------------
//
// The gate in `ServerStartup::phase_plan` used to reject any owner system that
// declared effects. The routing machinery (`runtime::owner_effects`,
// `effects::registered`) is implemented, so an extension-style system that
// declares `max_effects_per_tick > 0` must now register, emit through the
// frozen wake kind, and have its destination run next tick — never the same
// tick. Effects only schedule work sooner: every destination still does its
// own durable work through its normal handler.

/// Saturating owner counter that wakes `target` once, when `source` runs at
/// zero. Every run returns an emission patch so declared usage always matches
/// the emitted count.
fn saturating_wake_emitter(
    source: crate::server::parallel::OwnerKey,
    target: crate::server::entities::EntityId,
    runs: std::sync::Arc<std::sync::Mutex<Vec<crate::server::parallel::OwnerKey>>>,
    cap: u64,
) -> impl Fn(
    &crate::server::parallel::OwnerJob,
) -> Result<crate::server::parallel::OwnerPatch, crate::server::registry::SystemHandlerError>
+ Send
+ Sync {
    move |job: &crate::server::parallel::OwnerJob| {
        use crate::server::effects::EffectKindId;
        use crate::server::entities::wake::{EntityWake, WAKE_KIND_ID};
        use crate::server::parallel::{OwnerData, OwnerPatch, PatchUsage};
        use crate::server::registry::SystemHandlerError;
        use crate::server::runtime::owner_effects::{EmittedOwnerEffect, OwnerEffectPatch};
        runs.lock().unwrap().push(job.owner());
        let value = job
            .snapshot(job.owner())
            .and_then(|snapshot| snapshot.value::<OwnerData>())
            .and_then(|data| data.get::<u64>())
            .copied()
            .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
        let kind = EffectKindId::new(WAKE_KIND_ID).unwrap();
        let mut emissions = Vec::new();
        if job.owner() == source && value == 0 {
            emissions.push(EmittedOwnerEffect::new(kind, EntityWake { id: target }));
        }
        let effect_count = emissions.len();
        Ok(OwnerPatch::new(
            job,
            OwnerEffectPatch::new(OwnerData::new(value.saturating_add(1).min(cap)), emissions),
            PatchUsage {
                writes: 1,
                effects: effect_count,
                estimated_bytes: 8,
            },
        ))
    }
}

#[test]
fn startup_effect_emitting_owner_system_delivers_next_tick() {
    use crate::server::entities::EntityId;
    use crate::server::parallel::OwnerKey;
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};
    use std::sync::{Arc, Mutex};

    let system = SystemId::new("test:effect_owner").unwrap();
    let source = OwnerKey::Entity(1);
    let target = OwnerKey::Entity(2);
    let runs = Arc::new(Mutex::new(Vec::new()));
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Entity,
            1,
            8,
        )
        .write(ResourceId::new("test:effect_state").unwrap()),
        saturating_wake_emitter(
            source,
            EntityId::new(2).unwrap(),
            Arc::clone(&runs),
            u64::MAX,
        ),
    );
    register_u64_owner_codec(&mut startup, &system);
    startup.seed_owner(system.clone(), source, 0u64);
    startup.seed_owner(system.clone(), target, 0u64);

    let save = TestSave::new("startup-effect-owner-live");
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();

    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    // The producer committed, but the destination must not run in the same
    // tick: routed effects only schedule work for next tick.
    assert_eq!(*runs.lock().unwrap(), vec![source]);
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&system, source)
            .unwrap()
            .1,
        1
    );
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&system, target)
            .unwrap()
            .1,
        0
    );
    assert_eq!(state.system_runtime.pending_wake_count(), 1);

    tick_once(&mut state, TickId::new(2), Instant::now()).unwrap();
    assert_eq!(*runs.lock().unwrap(), vec![source, target]);
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&system, target)
            .unwrap()
            .1,
        1
    );
    assert_eq!(state.system_runtime.pending_wake_count(), 0);
}

#[test]
fn startup_still_rejects_unsupported_owner_capabilities() {
    use crate::server::parallel::{OwnerJob, OwnerPatch};
    use crate::server::registry::{
        OwnerPartition, ResourceId, SystemDescriptor, SystemHandlerError, SystemId,
    };

    fn rejected(descriptor: SystemDescriptor) -> std::io::Error {
        let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
        startup.register_system(
            descriptor,
            |_: &OwnerJob| -> Result<OwnerPatch, SystemHandlerError> {
                Err(SystemHandlerError::Rejected("unused".into()))
            },
        );
        startup.phase_plan().err().unwrap()
    }

    // Global-partition work is a coordinator adapter, not an owner batch.
    let error = rejected(
        SystemDescriptor::new(
            SystemId::new("test:global_owner").unwrap(),
            Phase::Simulation,
            OwnerPartition::Global,
            1,
            0,
        )
        .write(ResourceId::new("test:effect_state").unwrap()),
    );
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    assert!(
        error.to_string().contains("global partition"),
        "unexpected message: {error}"
    );

    // An owner system with no declared owner-state write is not a valid owner
    // system — even when it declares effects.
    let error = rejected(SystemDescriptor::new(
        SystemId::new("test:writeless_effects").unwrap(),
        Phase::Simulation,
        OwnerPartition::Entity,
        1,
        8,
    ));
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    assert!(
        error.to_string().contains("no owner-state write"),
        "unexpected message: {error}"
    );

    // Owner-system neighbour snapshots are not implemented.
    let error = rejected(
        SystemDescriptor::new(
            SystemId::new("test:neighbor_owner").unwrap(),
            Phase::Simulation,
            OwnerPartition::Chunk,
            1,
            0,
        )
        .neighbor_radius(1)
        .write(ResourceId::new("test:effect_state").unwrap()),
    );
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    assert!(
        error.to_string().contains("neighbor"),
        "unexpected message: {error}"
    );

    // The lifted case: an effect declaration with no other unsupported
    // capability now passes the startup gate.
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.register_system(
        SystemDescriptor::new(
            SystemId::new("test:effect_owner").unwrap(),
            Phase::Simulation,
            OwnerPartition::Entity,
            1,
            8,
        )
        .write(ResourceId::new("test:effect_state").unwrap()),
        |_: &OwnerJob| -> Result<OwnerPatch, SystemHandlerError> {
            Err(SystemHandlerError::Rejected("unused".into()))
        },
    );
    startup.phase_plan().unwrap();
}

#[test]
fn startup_effect_overflow_defers_without_failing_durability() {
    use crate::server::effects::EffectKindId;
    use crate::server::entities::EntityId;
    use crate::server::entities::wake::{EntityWake, WAKE_KIND_ID};
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};
    use crate::server::runtime::owner_effects::{EmittedOwnerEffect, OwnerEffectPatch};

    let system = SystemId::new("test:effect_overflow").unwrap();
    let owners = [OwnerKey::Entity(1), OwnerKey::Entity(2)];
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Entity,
            1,
            8,
        )
        .effects_per_job(1)
        .write(ResourceId::new("test:effect_state").unwrap()),
        move |job: &OwnerJob| {
            let kind = EffectKindId::new(WAKE_KIND_ID).unwrap();
            let emissions = vec![
                EmittedOwnerEffect::new(
                    kind.clone(),
                    EntityWake {
                        id: EntityId::new(1).unwrap(),
                    },
                ),
                EmittedOwnerEffect::new(
                    kind,
                    EntityWake {
                        id: EntityId::new(2).unwrap(),
                    },
                ),
            ];
            Ok(OwnerPatch::new(
                job,
                OwnerEffectPatch::new(OwnerData::new(1u64), emissions),
                PatchUsage {
                    writes: 1,
                    effects: 2,
                    estimated_bytes: 8,
                },
            ))
        },
    );
    register_u64_owner_codec(&mut startup, &system);
    for owner in owners {
        startup.seed_owner(system.clone(), owner, 0u64);
    }

    let save = TestSave::new("startup-effect-overflow");
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    let before = owners.map(|owner| {
        state
            .system_runtime
            .owner_value::<u64>(&system, owner)
            .unwrap()
    });
    let error = tick_once(&mut state, TickId::new(1), Instant::now()).unwrap_err();
    // Over-bound work defers; it never takes the coordinator's InvalidData
    // path, which would stop the coordinator.
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert!(!state.durability.failed);
    // Nothing committed: the over-bound producer output was rejected whole.
    assert_eq!(
        owners.map(|owner| {
            state
                .system_runtime
                .owner_value::<u64>(&system, owner)
                .unwrap()
        }),
        before
    );
    assert_eq!(state.system_runtime.pending_wake_count(), 0);
}

#[test]
fn startup_registered_effects_are_optional_for_convergence() {
    use crate::server::entities::EntityId;
    use crate::server::parallel::OwnerKey;
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};
    use std::sync::{Arc, Mutex};

    fn converge(drop_effects: bool) -> (Vec<[u64; 3]>, u64) {
        let system = SystemId::new("test:effect_converge").unwrap();
        let owners = [
            OwnerKey::Entity(1),
            OwnerKey::Entity(2),
            OwnerKey::Entity(3),
        ];
        let runs = Arc::new(Mutex::new(Vec::new()));
        let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
        startup.register_system(
            SystemDescriptor::new(
                system.clone(),
                Phase::Simulation,
                OwnerPartition::Entity,
                1,
                8,
            )
            .write(ResourceId::new("test:effect_state").unwrap()),
            saturating_wake_emitter(owners[0], EntityId::new(3).unwrap(), Arc::clone(&runs), 3),
        );
        register_u64_owner_codec(&mut startup, &system);
        for (owner, seed) in owners.iter().zip([0u64, 3, 0]) {
            startup.seed_owner(system.clone(), *owner, seed);
        }
        let save = TestSave::new("startup-effect-converge");
        let mut state =
            server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
        state
            .system_runtime
            .set_drop_registered_effects(drop_effects);
        let mut history = Vec::new();
        let mut target_saturated_at = 0;
        for tick in 1..=12u64 {
            tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
            let values = owners.map(|owner| {
                state
                    .system_runtime
                    .owner_value::<u64>(&system, owner)
                    .unwrap()
                    .1
            });
            if target_saturated_at == 0 && values[2] == 3 {
                target_saturated_at = tick;
            }
            history.push(values);
        }
        (history, target_saturated_at)
    }

    let (live_history, live_saturated) = converge(false);
    let (drop_history, drop_saturated) = converge(true);
    // Same final state with every effect dropped: a lost effect costs
    // latency, never state.
    assert_eq!(live_history.last(), Some(&[3, 3, 3]));
    assert_eq!(live_history.last(), drop_history.last());
    // ... but the woken owner gets there sooner with delivery live.
    assert!(
        live_saturated < drop_saturated,
        "live saturated at {live_saturated}, dropped at {drop_saturated}"
    );
}

// --- Durable owner state through the live path ---------------------------------
//
// The store below is the single source of truth: every owner wave commits as
// one main-journal transaction (staged before-values, one receipt, visibility
// only after the receipt), and recovery rebuilds it from the same
// `server.wal` latest-values map as every other domain. These tests prove the
// headline: a real registered owner system's state survives a real restart
// through `ServerStartup`, receipt-exact, with no second store involved.

/// A plain incrementing owner system with a fixed owner. Rebuilt identically
/// for every reopen so recovery — never the seed — supplies the state.
fn durable_counter_startup() -> (
    ServerStartup,
    crate::server::registry::SystemId,
    crate::server::parallel::OwnerKey,
) {
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};

    let system = SystemId::new("test:durable_counter").unwrap();
    let owner = OwnerKey::Entity(7);
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Entity,
            1,
            0,
        )
        .write(ResourceId::new("test:durable_counter_state").unwrap()),
        |job: &OwnerJob| {
            use crate::server::registry::SystemHandlerError;
            let value = job
                .snapshot(job.owner())
                .and_then(|snapshot| snapshot.value::<OwnerData>())
                .and_then(|data| data.get::<u64>())
                .copied()
                .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
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
    );
    register_u64_owner_codec(&mut startup, &system);
    startup.seed_owner(system.clone(), owner, 41u64);
    (startup, system, owner)
}

#[test]
fn registered_owner_state_survives_a_real_restart() {
    let save = TestSave::new("owner-durable-restart");
    let (startup, system, owner) = durable_counter_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    for tick in 1..=3u64 {
        tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
    }
    // Three waves, three receipts: the seed record plus one WAL record per
    // wave, nothing else staged on an idle server.
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owner),
        Some((3, 44))
    );
    assert_eq!(state.durability.writer.sequence(), 4);
    assert!(!state.durability.failed);
    // No clean shutdown: the runtime is simply dropped, the way a crashed
    // process leaves its WAL tail.
    drop(state);

    let (startup, system, owner) = durable_counter_startup();
    let mut reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    // Exactly the last acknowledged receipt's state: the seed replays as a
    // no-op and the three waves replay whole.
    assert_eq!(
        reopened.system_runtime.owner_value::<u64>(&system, owner),
        Some((3, 44))
    );
    // And the recovered store keeps ticking through the same journal.
    tick_once(&mut reopened, TickId::new(4), Instant::now()).unwrap();
    assert_eq!(
        reopened.system_runtime.owner_value::<u64>(&system, owner),
        Some((4, 45))
    );
}

#[test]
fn interrupted_owner_commit_recovers_to_the_last_complete_record() {
    use crate::server::journal::Transaction;
    use crate::server::parallel::OwnerData;
    use crate::server::runtime::owner_durable::OwnerWrite;

    let save = TestSave::new("owner-interrupted-commit");
    let (startup, system, owner) = durable_counter_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owner),
        Some((1, 42))
    );

    // Crash between WAL append and apply: the wave's record is synced (its
    // receipt is in hand) but the in-memory commit never runs.
    let (revision, _) = state
        .system_runtime
        .owner_value::<u64>(&system, owner)
        .unwrap();
    let prepared = state
        .system_runtime
        .prepare_owner_wave(
            &system,
            vec![OwnerWrite::new(owner, revision, OwnerData::new(43u64))],
        )
        .unwrap();
    let id = state.durability.next_id;
    state.durability.next_id = id.checked_add(1).expect("transaction IDs remain");
    let receiver = state
        .durability
        .writer
        .try_submit(Transaction::new(id, 2, prepared.changes().to_vec()))
        .unwrap();
    let receipt = receiver.recv().unwrap().unwrap();
    assert!(!receipt.duplicate);
    drop(prepared);
    drop(state);

    // Recovery lands on the last complete record: the interrupted wave's
    // after-value is durable even though the crashed process never applied
    // it, and there is no half-applied state.
    let (startup, system, owner) = durable_counter_startup();
    let mut reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    assert_eq!(
        reopened.system_runtime.owner_value::<u64>(&system, owner),
        Some((2, 43))
    );
    tick_once(&mut reopened, TickId::new(3), Instant::now()).unwrap();
    assert_eq!(
        reopened.system_runtime.owner_value::<u64>(&system, owner),
        Some((3, 44))
    );
}

/// A two-owner incrementing system: one wave covers both owners so an
/// interrupted commit must recover both or neither.
fn durable_pair_startup() -> (
    ServerStartup,
    crate::server::registry::SystemId,
    [crate::server::parallel::OwnerKey; 2],
) {
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};

    let system = SystemId::new("test:durable_pair").unwrap();
    let owners = [OwnerKey::Entity(7), OwnerKey::Entity(8)];
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Entity,
            2,
            0,
        )
        .write(ResourceId::new("test:durable_pair_state").unwrap()),
        |job: &OwnerJob| {
            use crate::server::registry::SystemHandlerError;
            let value = job
                .snapshot(job.owner())
                .and_then(|snapshot| snapshot.value::<OwnerData>())
                .and_then(|data| data.get::<u64>())
                .copied()
                .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
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
    );
    register_u64_owner_codec(&mut startup, &system);
    startup.seed_owner(system.clone(), owners[0], 41u64);
    startup.seed_owner(system.clone(), owners[1], 100u64);
    (startup, system, owners)
}

#[test]
fn interrupted_multi_owner_wave_recovers_atomically() {
    use crate::server::journal::Transaction;
    use crate::server::parallel::OwnerData;
    use crate::server::runtime::owner_durable::OwnerWrite;

    // Crash after the receipt but before the apply: the two-owner record is
    // synced whole, so recovery lands on the last complete record with both
    // owners advanced — never one without the other.
    let save = TestSave::new("owner-interrupted-pair-receipted");
    let (startup, system, owners) = durable_pair_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[0]),
        Some((1, 42))
    );
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[1]),
        Some((1, 101))
    );
    let prepared = state
        .system_runtime
        .prepare_owner_wave(
            &system,
            vec![
                OwnerWrite::new(owners[0], 1, OwnerData::new(43u64)),
                OwnerWrite::new(owners[1], 1, OwnerData::new(102u64)),
            ],
        )
        .unwrap();
    assert_eq!(prepared.changes().len(), 2);
    let id = state.durability.next_id;
    state.durability.next_id = id.checked_add(1).expect("transaction IDs remain");
    let receiver = state
        .durability
        .writer
        .try_submit(Transaction::new(id, 2, prepared.changes().to_vec()))
        .unwrap();
    let receipt = receiver.recv().unwrap().unwrap();
    assert!(!receipt.duplicate);
    drop(prepared);
    drop(state);

    let (startup, system, owners) = durable_pair_startup();
    let reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    assert_eq!(
        reopened
            .system_runtime
            .owner_value::<u64>(&system, owners[0]),
        Some((2, 43))
    );
    assert_eq!(
        reopened
            .system_runtime
            .owner_value::<u64>(&system, owners[1]),
        Some((2, 102))
    );
}

#[test]
fn interrupted_multi_owner_wave_before_sync_recovers_whole_or_nothing() {
    use crate::server::journal::Transaction;
    use crate::server::parallel::OwnerData;
    use crate::server::runtime::owner_durable::OwnerWrite;

    // Crash between stage and receipt: the record may or may not have synced
    // before the process died. Either way recovery lands on a complete
    // record — both owners old or both owners new — never a half-applied
    // wave. The assertion holds regardless of scheduling, so it is not a
    // timing test.
    let save = TestSave::new("owner-interrupted-pair-staged");
    let (startup, system, owners) = durable_pair_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    let prepared = state
        .system_runtime
        .prepare_owner_wave(
            &system,
            vec![
                OwnerWrite::new(owners[0], 1, OwnerData::new(43u64)),
                OwnerWrite::new(owners[1], 1, OwnerData::new(102u64)),
            ],
        )
        .unwrap();
    let id = state.durability.next_id;
    state.durability.next_id = id.checked_add(1).expect("transaction IDs remain");
    let _receiver = state
        .durability
        .writer
        .try_submit(Transaction::new(id, 2, prepared.changes().to_vec()))
        .unwrap();
    drop(prepared);
    drop(state);

    let (startup, system, owners) = durable_pair_startup();
    let reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    let first = reopened
        .system_runtime
        .owner_value::<u64>(&system, owners[0]);
    let second = reopened
        .system_runtime
        .owner_value::<u64>(&system, owners[1]);
    assert!(
        (first == Some((1, 42)) && second == Some((1, 101)))
            || (first == Some((2, 43)) && second == Some((2, 102))),
        "recovery must land on a complete record, got {first:?} and {second:?}"
    );
}

#[test]
fn entity_and_owner_state_commit_as_one_atomic_record() {
    use crate::server::durable::CommitAction;
    use crate::server::entities::{EntityPayload, EntitySpawn};
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};
    use crate::server::startup::StartupEntityType;

    let entity_id = crate::content::EntityTypeId(70_021);
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: entity_id,
            key: "test:combo_entity".into(),
            schema_version: 1,
            schema_fingerprint: 0xC0AB_0001,
        })
        .unwrap();
    let system = SystemId::new("test:combo_owner").unwrap();
    let owner = OwnerKey::Entity(99);
    let build_startup = || {
        let mut startup = ServerStartup::new(Arc::new(catalog.clone()));
        startup.register_entity_type(StartupEntityType {
            key: "test:combo_entity".into(),
            ownership: crate::server::entities::EntityOwnership::Mobile,
            tick_policy: crate::server::entities::TickPolicy::Never,
            max_payload_bytes: 1,
            codec: Arc::new(StartupProbeCodec),
            interaction_policy: None,
            tick_planner: None,
        });
        startup.register_system(
            SystemDescriptor::new(
                system.clone(),
                Phase::Simulation,
                OwnerPartition::Entity,
                1,
                0,
            )
            .write(ResourceId::new("test:combo_owner_state").unwrap()),
            |job: &OwnerJob| {
                use crate::server::registry::SystemHandlerError;
                let value = job
                    .snapshot(job.owner())
                    .and_then(|snapshot| snapshot.value::<OwnerData>())
                    .and_then(|data| data.get::<u64>())
                    .copied()
                    .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
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
        );
        register_u64_owner_codec(&mut startup, &system);
        startup
    };

    let save = TestSave::new("owner-entity-atomic-commit");
    let mut state =
        server_state_with_startup(7, save.path().to_path_buf(), 1, build_startup()).unwrap();
    // One prepared transaction spans both domains: the entity spawn plus a
    // fresh owner cell, joined through `add_related_change` into a single
    // WAL record. Replay can only ever apply the record whole.
    let mut batch = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: entity_id,
            position: [0.5, 80.0, 0.5],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let spawned = batch.entity_id();
    let owner_change = state
        .system_runtime
        .stage_owner_insert(&system, owner, &OwnerData::new(9u64))
        .unwrap();
    batch.add_related_change(owner_change).unwrap();
    assert!(
        batch
            .changes()
            .iter()
            .any(|change| change.key.domain == "bloxgloom:entity"),
        "the transaction carries entity state"
    );
    assert!(
        batch
            .changes()
            .iter()
            .any(|change| change.key.domain == "bloxgloom:owner_state"),
        "the transaction carries owner state"
    );
    // The spawn stages through the WAL so the entity checkpoint mirror
    // replays the same batch as the live store.
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the combined spawn");
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(batch),
        entity_wakes: Vec::new(),
    };
    assert!(
        state
            .durability
            .try_stage(
                crate::server::simulation::TickId::new(1),
                &action,
                Some(permit)
            )
            .unwrap()
    );
    for _ in 0..2_000 {
        crate::server::durable::process_durable_actions(
            &mut state,
            crate::server::simulation::TickId::new(1),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        state.durability.pending.is_empty(),
        "combined commit must apply"
    );
    // Both ends applied together through the real coordinator: no half-apply.
    assert_eq!(
        state
            .entities
            .snapshot(spawned)
            .unwrap()
            .private_payload
            .downcast_ref::<u8>(),
        Some(&7u8)
    );
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owner),
        Some((0, 9))
    );
    drop(state);

    let reopened =
        server_state_with_startup(7, save.path().to_path_buf(), 1, build_startup()).unwrap();
    // The single record replayed whole: entity and owner state agree after
    // restart, with no reconciliation between two tails.
    assert_eq!(
        reopened
            .entities
            .snapshot(spawned)
            .unwrap()
            .private_payload
            .downcast_ref::<u8>(),
        Some(&7u8)
    );
    assert_eq!(
        reopened.system_runtime.owner_value::<u64>(&system, owner),
        Some((0, 9))
    );
}

#[test]
fn owner_state_survives_journal_rotation_and_bounds_the_tail() {
    let save = TestSave::new("owner-rotation-tail");
    let (startup, _, _) = durable_counter_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    for tick in 1..=3u64 {
        tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
    }
    assert!(state.durability.pending.is_empty());
    let tail_before = state.durability.writer.bytes();
    assert!(tail_before > 0);

    // Rotation materializes the full latest-value map — every domain,
    // including `bloxgloom:owner_state` — into the new base generation, so
    // the truncated tail stays bounded without a per-key checkpoint file.
    state.durability.force_rotation_at_sequence = Some(state.durability.writer.sequence());
    for _ in 0..2_000 {
        crate::server::durable::process_durable_actions(&mut state, TickId::new(4), Instant::now())
            .unwrap();
        if state.durability.completed_rotations == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(state.durability.completed_rotations, 1);
    assert!(
        state.durability.writer.bytes() < tail_before,
        "rotation must truncate the tail that carried the owner waves"
    );
    drop(state);

    let (startup, system, owner) = durable_counter_startup();
    let reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    assert_eq!(
        reopened.system_runtime.owner_value::<u64>(&system, owner),
        Some((3, 44))
    );
}

#[test]
fn stale_owner_reads_reject_the_wave_on_the_live_path() {
    use crate::server::parallel::OwnerData;
    use crate::server::runtime::owner_durable::{OwnerDurableError, OwnerWrite};

    let save = TestSave::new("owner-stale-live");
    let (startup, system, owner) = durable_counter_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owner),
        Some((1, 42))
    );

    // A wave prepared against revision 0 is stale once the live store is at
    // revision 1. The whole wave rejects before anything is staged: no WAL
    // record, no partial apply, and the coordinator keeps running.
    let sequence_before = state.durability.writer.sequence();
    let error = state
        .system_runtime
        .prepare_owner_wave(
            &system,
            vec![OwnerWrite::new(owner, 0, OwnerData::new(999u64))],
        )
        .unwrap_err();
    assert!(
        matches!(error, OwnerDurableError::StaleRevision { .. }),
        "stale reads must reject the wave, got {error:?}"
    );
    assert_eq!(state.durability.writer.sequence(), sequence_before);
    assert!(!state.durability.failed);
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owner),
        Some((1, 42))
    );
    tick_once(&mut state, TickId::new(2), Instant::now()).unwrap();
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owner),
        Some((2, 43))
    );
}

#[test]
fn oversized_owner_values_defer_without_failing_durability() {
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};
    use crate::server::runtime::owner_codec::{OwnerCodecError, OwnerValueCodec};

    struct BlobCodec;

    impl OwnerValueCodec for BlobCodec {
        fn decode(&self, payload: &[u8]) -> Result<OwnerData, OwnerCodecError> {
            Ok(OwnerData::new(payload.to_vec()))
        }

        fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError> {
            value
                .get::<Vec<u8>>()
                .cloned()
                .ok_or(OwnerCodecError::InvalidData)
        }
    }

    let system = SystemId::new("test:blob_owner").unwrap();
    let owner = OwnerKey::Entity(11);
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Entity,
            1,
            0,
        )
        .write(ResourceId::new("test:blob_owner_state").unwrap()),
        |job: &OwnerJob| {
            Ok(OwnerPatch::new(
                job,
                OwnerData::new(vec![0u8; 64]),
                PatchUsage {
                    writes: 1,
                    effects: 0,
                    estimated_bytes: 64,
                },
            ))
        },
    );
    startup.register_owner_codec(
        system.clone(),
        crate::server::startup::StartupOwnerCodec {
            codec: Arc::new(BlobCodec),
            codec_version: 1,
            max_bytes: 8,
        },
    );
    startup.seed_owner(system.clone(), owner, vec![1u8; 8]);

    let save = TestSave::new("owner-capacity-live");
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    let sequence_before = state.durability.writer.sequence();
    let error = tick_once(&mut state, TickId::new(1), Instant::now()).unwrap_err();
    // Capacity defers one owner; it never takes the coordinator's
    // `InvalidData` path, which would close every client socket.
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert_ne!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(!state.durability.failed);
    // Nothing staged and nothing committed: no WAL record was written and
    // the live cell is untouched.
    assert_eq!(state.durability.writer.sequence(), sequence_before);
    assert_eq!(
        state
            .system_runtime
            .owner_value::<Vec<u8>>(&system, owner)
            .unwrap(),
        (0, vec![1u8; 8])
    );
}
// --- Tampered multi-domain commits -----------------------------------------------
//
// One WAL record carries both halves of a combined entity+owner transaction.
// These tests tamper with the owner half after staging to prove both
// enforcement points: the journal worker rechecks every before-value against
// committed history before appending, and the apply path rechecks them again
// before mutating memory. Genuine corruption stops the coordinator; it never
// half-applies.

/// Startup with one mobile probe entity and one u64 owner cell, plus the
/// handles both halves' tests need to rebuild it for reopen.
fn tamper_startup() -> (
    ServerStartup,
    crate::content::EntityTypeId,
    crate::server::registry::SystemId,
    crate::server::parallel::OwnerKey,
) {
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};
    use crate::server::startup::StartupEntityType;

    let entity_type = crate::content::EntityTypeId(70_022);
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: entity_type,
            key: "test:tamper_entity".into(),
            schema_version: 1,
            schema_fingerprint: 0x7A_F00D_0001,
        })
        .unwrap();
    let system = SystemId::new("test:tamper_owner").unwrap();
    let owner = OwnerKey::Entity(13);
    let mut startup = ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:tamper_entity".into(),
        ownership: crate::server::entities::EntityOwnership::Mobile,
        tick_policy: crate::server::entities::TickPolicy::Never,
        max_payload_bytes: 1,
        codec: Arc::new(StartupProbeCodec),
        interaction_policy: None,
        tick_planner: None,
    });
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Entity,
            1,
            0,
        )
        .write(ResourceId::new("test:tamper_owner_state").unwrap()),
        |job: &OwnerJob| {
            use crate::server::registry::SystemHandlerError;
            let value = job
                .snapshot(job.owner())
                .and_then(|snapshot| snapshot.value::<OwnerData>())
                .and_then(|data| data.get::<u64>())
                .copied()
                .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
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
    );
    register_u64_owner_codec(&mut startup, &system);
    startup.seed_owner(system.clone(), owner, 0u64);
    (startup, entity_type, system, owner)
}

/// Stages one entity batch through the WAL — with its checkpoint-mirror
/// reservation — and settles it, so the live store and the mirror advance
/// together.
fn stage_tamper_batch(
    state: &mut State,
    batch: crate::server::entities::PreparedEntityBatch,
    tick: u64,
) {
    use crate::server::durable::CommitAction;

    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the batch");
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(batch),
        entity_wakes: Vec::new(),
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(tick), &action, Some(permit))
            .unwrap()
    );
    for _ in 0..2_000 {
        crate::server::durable::process_durable_actions(state, TickId::new(tick), Instant::now())
            .unwrap();
        if state.durability.pending.is_empty() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("tamper batch must settle");
}

/// Settles until the coordinator stops: a commit the journal refused or an
/// apply that failed must surface as an error, never as a quiet settle.
fn settle_until_fatal(state: &mut State, tick: u64) {
    for _ in 0..2_000 {
        match crate::server::durable::process_durable_actions(
            state,
            TickId::new(tick),
            Instant::now(),
        ) {
            Ok(()) if state.durability.pending.is_empty() => {
                panic!("tampered commit must not settle")
            }
            Ok(()) => {}
            Err(_) => return,
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("tampered commit must stop the coordinator");
}

/// Spawns the probe entity through the WAL and returns its ID.
fn spawn_tamper_entity(
    state: &mut State,
    entity_type: crate::content::EntityTypeId,
) -> crate::server::entities::EntityId {
    use crate::server::entities::{EntityPayload, EntitySpawn};

    let prepared = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type,
            position: [0.5, 80.0, 0.5],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let id = prepared.entity_id();
    stage_tamper_batch(state, prepared, 1);
    id
}

/// Prepares the probe update (payload 7 to 8) with a prepared owner wave
/// joined in, so one record spans both domains.
fn tampered_update(
    state: &mut State,
    spawned: crate::server::entities::EntityId,
    system: &crate::server::registry::SystemId,
    owner: crate::server::parallel::OwnerKey,
    tamper: impl FnOnce(&mut crate::server::journal::Change),
) -> crate::server::entities::PreparedEntityBatch {
    use crate::server::entities::{EntityPatch, EntityPayload};
    use crate::server::parallel::OwnerData;
    use crate::server::runtime::owner_durable::OwnerWrite;

    let snapshot = state.entities.snapshot(spawned).unwrap();
    let mut update = state
        .entities
        .prepare_update(
            spawned,
            snapshot.revision,
            EntityPatch {
                payload: Some(EntityPayload::new(8u8)),
                next_tick: None,
                position: None,
            },
        )
        .unwrap();
    let wave = state
        .system_runtime
        .prepare_owner_wave(
            system,
            vec![OwnerWrite::new(owner, 0, OwnerData::new(1u64))],
        )
        .unwrap();
    let mut change = wave.changes()[0].clone();
    tamper(&mut change);
    update.add_related_change(change).unwrap();
    update
}

#[test]
fn tampered_owner_before_values_reject_the_whole_record() {
    let save = TestSave::new("owner-tamper-before");
    let (startup, entity_type, system, owner) = tamper_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    let spawned = spawn_tamper_entity(&mut state, entity_type);
    let update = tampered_update(&mut state, spawned, &system, owner, |change| {
        change.before = vec![0xFF];
    });
    // The journal worker rechecks every before-value against committed
    // history before appending, so the whole record is rejected: the entity
    // half cannot slip through without the owner half. The coordinator stops
    // with nothing committed anywhere.
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the update");
    let action = tamper_action(update);
    assert!(
        state
            .durability
            .try_stage(TickId::new(2), &action, Some(permit))
            .unwrap()
    );
    settle_until_fatal(&mut state, 2);
    assert!(state.durability.failed);
    assert_eq!(tamper_payload(&state, spawned), Some(7u8));
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owner),
        Some((0, 0))
    );
    drop(state);

    // Restart agrees: neither half is durable, so both still hold their
    // pre-transaction state. Atomicity holds before and after the crash.
    let (startup, _, system, owner) = tamper_startup();
    let reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    assert_eq!(tamper_payload(&reopened, spawned), Some(7u8));
    assert_eq!(
        reopened.system_runtime.owner_value::<u64>(&system, owner),
        Some((0, 0))
    );
}

#[test]
fn undecodable_owner_after_values_stop_recovery_fail_closed() {
    let save = TestSave::new("owner-tamper-after");
    let (startup, entity_type, system, owner) = tamper_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    let spawned = spawn_tamper_entity(&mut state, entity_type);
    // A tampered after-value passes the journal (before-values match) but
    // cannot decode at apply. The apply fails fatal in memory, and reopening
    // refuses the world fail-closed instead of running on undecodable state.
    let update = tampered_update(&mut state, spawned, &system, owner, |change| {
        change.after = vec![0xAA; 8];
    });
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the update");
    let action = tamper_action(update);
    assert!(
        state
            .durability
            .try_stage(TickId::new(2), &action, Some(permit))
            .unwrap()
    );
    settle_until_fatal(&mut state, 2);
    assert!(state.durability.failed);
    drop(state);

    let (startup, _, _, _) = tamper_startup();
    let error = server_state_with_startup(7, save.path().to_path_buf(), 1, startup)
        .err()
        .expect("reopening undecodable owner state must fail");
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

/// Wraps a prepared entity batch (possibly spanning owner state) for staging.
fn tamper_action(
    batch: crate::server::entities::PreparedEntityBatch,
) -> crate::server::durable::CommitAction {
    use crate::server::durable::CommitAction;

    CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(batch),
        entity_wakes: Vec::new(),
    }
}

/// Private entity payload byte for the tamper probe.
fn tamper_payload(state: &State, id: crate::server::entities::EntityId) -> Option<u8> {
    state
        .entities
        .snapshot(id)
        .unwrap()
        .private_payload
        .downcast_ref::<u8>()
        .copied()
}

// --- Durable pending owner wakes across restart --------------------------------
//
// An effect may only cause work to happen SOONER: the durable truth is the
// schedule, not the effect. A wake to an owner with no live cell is therefore
// held as a tiny bounded due-flag keyed by destination — no effect payload
// persists — staged in the producer wave's own WAL record and served when the
// destination loads. These tests prove the headline: emit, restart without
// delivering, load the destination, and it runs exactly once.

/// One producer owner, one bystander, plus one never-seeded destination. The
/// producer emits a `bloxgloom:wake_entity` notification at its first run;
/// the destination is absent until a later startup seeds it. All counters
/// saturate so schedules can differ while final states match. Rebuilt
/// identically for every reopen so recovery — never the seed — supplies the
/// flag.
fn wake_flag_startup(
    seed_destination: bool,
) -> (
    ServerStartup,
    crate::server::registry::SystemId,
    crate::server::parallel::OwnerKey,
    crate::server::parallel::OwnerKey,
) {
    use crate::server::effects::EffectKindId;
    use crate::server::entities::wake::{EntityWake, WAKE_KIND_ID};
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};
    use crate::server::runtime::owner_effects::EmittedOwnerEffect;

    let system = SystemId::new("test:wake_flag").unwrap();
    let producer = OwnerKey::Entity(1);
    let bystander = OwnerKey::Entity(2);
    let destination = OwnerKey::Entity(999);
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Entity,
            1,
            8,
        )
        .effects_per_job(4)
        .write(ResourceId::new("test:wake_flag_state").unwrap()),
        move |job: &OwnerJob| {
            use crate::server::registry::SystemHandlerError;
            let value = job
                .snapshot(job.owner())
                .and_then(|snapshot| snapshot.value::<OwnerData>())
                .and_then(|data| data.get::<u64>())
                .copied()
                .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
            let mut emissions = Vec::new();
            if job.owner() == producer && value == 0 {
                emissions.push(EmittedOwnerEffect::new(
                    EffectKindId::new(WAKE_KIND_ID).expect("static wake kind"),
                    EntityWake {
                        id: crate::server::entities::EntityId::new(999).expect("nonzero"),
                    },
                ));
            }
            let usage = PatchUsage {
                writes: 1,
                effects: emissions.len(),
                estimated_bytes: std::mem::size_of::<u64>(),
            };
            // Saturating counters: schedules may differ, final states match.
            let updated = value.saturating_add(1).min(3);
            Ok(OwnerPatch::new(
                job,
                crate::server::runtime::owner_effects::OwnerEffectPatch::new(
                    OwnerData::new(updated),
                    emissions,
                ),
                usage,
            ))
        },
    );
    register_u64_owner_codec(&mut startup, &system);
    startup.seed_owner(system.clone(), producer, 0u64);
    startup.seed_owner(system.clone(), bystander, 0u64);
    if seed_destination {
        startup.seed_owner(system.clone(), destination, 0u64);
    }
    (startup, system, producer, destination)
}

#[test]
fn wake_to_an_unloaded_owner_survives_restart_and_runs_once() {
    let save = TestSave::new("owner-wake-restart");
    let (startup, system, producer, _destination) = wake_flag_startup(false);
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    // The producer committed; the destination was never resident, so its wake
    // is held durably instead of being skipped or staged live.
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, producer),
        Some((1, 1))
    );
    assert_eq!(state.system_runtime.durable_wake_count(), 1);
    assert_eq!(state.system_runtime.pending_wake_count(), 0);
    assert!(!state.durability.failed);
    // No clean shutdown and no delivery: the runtime is dropped with the
    // destination still absent, the way a crash leaves the flag staged.
    drop(state);

    // Reload with the destination seeded: the flag recovers, the seed loads
    // the cell through the WAL, and the very next tick serves the flagged
    // destination ahead of rotation — rotation alone would run Entity(1).
    let (startup, system, producer, destination) = wake_flag_startup(true);
    let mut reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    assert_eq!(reopened.system_runtime.durable_wake_count(), 1);
    tick_once(&mut reopened, TickId::new(2), Instant::now()).unwrap();
    // The destination ran its own durable work ...
    assert_eq!(
        reopened
            .system_runtime
            .owner_value::<u64>(&system, destination),
        Some((1, 1))
    );
    // ... while the producer waited: the flag, not rotation, scheduled it.
    assert_eq!(
        reopened
            .system_runtime
            .owner_value::<u64>(&system, producer),
        Some((1, 1))
    );
    // Serving cleared the flag in the destination wave's own record.
    assert_eq!(reopened.system_runtime.durable_wake_count(), 0);
    assert!(!reopened.durability.failed);
    drop(reopened);

    // A further restart replays no wake: the destination runs only on its own
    // rotation now, exactly once across replays, never once per replay. The
    // wake-only wave did not jump the ordinary cursor over the bystander.
    let (startup, system, producer, destination) = wake_flag_startup(true);
    let mut replayed = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    assert_eq!(replayed.system_runtime.durable_wake_count(), 0);
    tick_once(&mut replayed, TickId::new(3), Instant::now()).unwrap();
    assert_eq!(
        replayed
            .system_runtime
            .owner_value::<u64>(&system, producer),
        Some((1, 1))
    );
    assert_eq!(
        replayed
            .system_runtime
            .owner_value::<u64>(&system, destination),
        Some((1, 1))
    );
    assert_eq!(replayed.system_runtime.durable_wake_count(), 0);
    tick_once(&mut replayed, TickId::new(4), Instant::now()).unwrap();
    assert_eq!(
        replayed
            .system_runtime
            .owner_value::<u64>(&system, destination),
        Some((2, 2))
    );
    tick_once(&mut replayed, TickId::new(5), Instant::now()).unwrap();
    assert_eq!(
        replayed
            .system_runtime
            .owner_value::<u64>(&system, producer),
        Some((2, 2))
    );
    assert!(!replayed.durability.failed);
}

#[test]
fn dropped_wake_flags_delay_but_do_not_change_the_outcome() {
    // Same startup with every owner seeded throughout: with delivery live
    // the flagged destination runs ahead of rotation; with every effect
    // dropped before delivery nothing is staged and rotation still converges
    // all three saturating counters to the same final state.
    fn converge(drop: bool) -> (Vec<[u64; 3]>, u64) {
        let save = TestSave::new("owner-wake-drop");
        let (startup, system, producer, destination) = wake_flag_startup(true);
        let mut state =
            server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
        state.system_runtime.set_drop_registered_effects(drop);
        let bystander = crate::server::parallel::OwnerKey::Entity(2);
        let mut history = Vec::new();
        let mut destination_saturated_at = 0;
        for tick in 1..=12u64 {
            tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
            let values = [
                state
                    .system_runtime
                    .owner_value::<u64>(&system, producer)
                    .unwrap()
                    .1,
                state
                    .system_runtime
                    .owner_value::<u64>(&system, bystander)
                    .unwrap()
                    .1,
                state
                    .system_runtime
                    .owner_value::<u64>(&system, destination)
                    .unwrap()
                    .1,
            ];
            if destination_saturated_at == 0 && values[2] == 3 {
                destination_saturated_at = tick;
            }
            history.push(values);
        }
        (history, destination_saturated_at)
    }

    let (live_history, live_saturated) = converge(false);
    let (drop_history, drop_saturated) = converge(true);
    assert_eq!(live_history.last(), Some(&[3, 3, 3]));
    assert_eq!(live_history.last(), drop_history.last());
    // The woken destination saturates sooner with delivery live ...
    assert!(
        live_saturated < drop_saturated,
        "live saturated at {live_saturated}, dropped at {drop_saturated}"
    );
    // ... because tick two serves it while rotation alone serves the
    // bystander.
    assert_eq!(live_history[1], [1, 0, 1]);
    assert_eq!(drop_history[1], [1, 1, 0]);
}

/// Three plain incrementing owners on a one-job budget. Rebuilt identically
/// for every reopen so recovery — never the seed — supplies the cursor.
fn rotation_cursor_startup() -> (
    ServerStartup,
    crate::server::registry::SystemId,
    [crate::server::parallel::OwnerKey; 3],
) {
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};

    let system = SystemId::new("test:rotation_cursor").unwrap();
    let owners = [
        OwnerKey::Entity(1),
        OwnerKey::Entity(2),
        OwnerKey::Entity(3),
    ];
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Entity,
            1,
            0,
        )
        .write(ResourceId::new("test:rotation_cursor_state").unwrap()),
        |job: &OwnerJob| {
            use crate::server::registry::SystemHandlerError;
            let value = job
                .snapshot(job.owner())
                .and_then(|snapshot| snapshot.value::<OwnerData>())
                .and_then(|data| data.get::<u64>())
                .copied()
                .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
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
    );
    register_u64_owner_codec(&mut startup, &system);
    for owner in owners {
        startup.seed_owner(system.clone(), owner, 0u64);
    }
    (startup, system, owners)
}

#[test]
fn rotation_cursor_survives_restart_without_starving() {
    let save = TestSave::new("owner-cursor-restart");
    let (startup, system, owners) = rotation_cursor_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    tick_once(&mut state, TickId::new(2), Instant::now()).unwrap();
    // Round-robin from the lowest owner served each exactly once.
    let values = |state: &State| {
        owners.map(|owner| {
            state
                .system_runtime
                .owner_value::<u64>(&system, owner)
                .unwrap()
                .1
        })
    };
    assert_eq!(values(&state), [1, 1, 0]);
    assert!(!state.durability.failed);
    // No clean shutdown: the cursor's last receipted record is the truth.
    drop(state);

    // The rotation resumes after its last served owner instead of restarting
    // at the lowest: the previously starved owner runs next, not last.
    let (startup, _, _) = rotation_cursor_startup();
    let mut reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    tick_once(&mut reopened, TickId::new(3), Instant::now()).unwrap();
    assert_eq!(values(&reopened), [1, 1, 1]);
    // And the rotation keeps wrapping fairly from there.
    tick_once(&mut reopened, TickId::new(4), Instant::now()).unwrap();
    assert_eq!(values(&reopened), [2, 1, 1]);
    assert!(!reopened.durability.failed);
}

#[test]
fn pending_wakes_and_cursors_survive_rotation_without_wedging_the_gate() {
    let save = TestSave::new("owner-wake-rotation");
    let (startup, _, _, _) = wake_flag_startup(false);
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    assert_eq!(state.system_runtime.durable_wake_count(), 1);
    // The tripwire: owner-wave commits register no per-key checkpoint work,
    // so the rotation gate can drain. A dirty entry without a backing file
    // would wedge rotation forever.
    assert!(state.durability.dirty_checkpoints.is_empty());
    assert!(state.durability.pending.is_empty());
    let tail_before = state.durability.writer.bytes();
    assert!(tail_before > 0);

    // Rotation materializes the full latest-value map — owner cells, wake
    // flags, and cursors alike — into the new base generation.
    state.durability.force_rotation_at_sequence = Some(state.durability.writer.sequence());
    for _ in 0..2_000 {
        crate::server::durable::process_durable_actions(&mut state, TickId::new(2), Instant::now())
            .unwrap();
        if state.durability.completed_rotations == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(state.durability.completed_rotations, 1);
    assert!(
        state.durability.writer.bytes() < tail_before,
        "rotation must truncate the tail that carried the owner wave"
    );
    assert_eq!(state.system_runtime.durable_wake_count(), 1);
    drop(state);

    // The flag and the producer's work are intact past the rotation.
    let (startup, system, producer, destination) = wake_flag_startup(true);
    let mut reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    assert_eq!(reopened.system_runtime.durable_wake_count(), 1);
    assert_eq!(
        reopened
            .system_runtime
            .owner_value::<u64>(&system, producer),
        Some((1, 1))
    );
    tick_once(&mut reopened, TickId::new(3), Instant::now()).unwrap();
    assert_eq!(
        reopened
            .system_runtime
            .owner_value::<u64>(&system, destination),
        Some((1, 1))
    );
    assert_eq!(reopened.system_runtime.durable_wake_count(), 0);
    assert!(!reopened.durability.failed);
}

/// Two incrementing systems with one owner each: every live tick stages two
/// waves, so a crash can catch several waves in flight at once.
fn durable_twin_startup() -> (
    ServerStartup,
    [crate::server::registry::SystemId; 2],
    [crate::server::parallel::OwnerKey; 2],
) {
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};

    let systems = [
        SystemId::new("test:durable_twin_a").unwrap(),
        SystemId::new("test:durable_twin_b").unwrap(),
    ];
    let owners = [OwnerKey::Entity(7), OwnerKey::Entity(8)];
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    for ((system, owner), seed) in systems.iter().zip(owners.iter()).zip([41u64, 100]) {
        startup.register_system(
            SystemDescriptor::new(
                system.clone(),
                Phase::Simulation,
                OwnerPartition::Entity,
                1,
                0,
            )
            .write(
                ResourceId::new(format!("test:{}_state", system.as_str().replace(':', "_")))
                    .unwrap(),
            ),
            |job: &OwnerJob| {
                use crate::server::registry::SystemHandlerError;
                let value = job
                    .snapshot(job.owner())
                    .and_then(|snapshot| snapshot.value::<OwnerData>())
                    .and_then(|data| data.get::<u64>())
                    .copied()
                    .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
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
        );
        register_u64_owner_codec(&mut startup, system);
        startup.seed_owner(system.clone(), *owner, seed);
    }
    (startup, systems, owners)
}

#[test]
fn interrupted_multi_wave_commit_recovers_whole_waves_only() {
    // Two systems commit through the live loop, then two more waves are
    // staged through the production path and only the first is polled before
    // the crash. The second record was submitted but never polled; the writer
    // drains it to the WAL tail on shutdown, so recovery must contain both
    // waves whole — never a mix, never half-applied, never doubled.
    let save = TestSave::new("owner-interrupted-twin-waves");
    let (startup, systems, owners) = durable_twin_startup();
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&systems[0], owners[0]),
        Some((1, 42))
    );
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&systems[1], owners[1]),
        Some((1, 101))
    );

    let kinds = Arc::clone(&state.effect_kinds);
    let first = state.phase_plan.system(&systems[0]).unwrap().clone();
    let second = state.phase_plan.system(&systems[1]).unwrap().clone();
    let wave_a = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_registered_wave(&first, TickId::new(2), 0, &kinds, durability, &[])
            .unwrap()
            .expect("twin A stages")
    };
    let wave_b = {
        let (runtime, durability) = (&mut state.system_runtime, &mut state.durability);
        runtime
            .stage_registered_wave(
                &second,
                TickId::new(2),
                1,
                &kinds,
                durability,
                &[wave_a.keys().to_vec()],
            )
            .unwrap()
            .expect("disjoint twin B stages while A is in flight")
    };
    // Neither staged wave is visible before its receipt.
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&systems[0], owners[0]),
        Some((1, 42))
    );
    assert_eq!(
        state
            .system_runtime
            .owner_value::<u64>(&systems[1], owners[1]),
        Some((1, 101))
    );
    // Crash after applying only the first wave.
    assert_eq!(
        crate::server::durable::complete_barrier(&mut state, wave_a.barrier())
            .unwrap()
            .commits,
        1
    );
    drop(wave_b);
    drop(state);

    let (startup, systems, owners) = durable_twin_startup();
    let reopened = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    assert_eq!(
        reopened
            .system_runtime
            .owner_value::<u64>(&systems[0], owners[0]),
        Some((2, 43))
    );
    assert_eq!(
        reopened
            .system_runtime
            .owner_value::<u64>(&systems[1], owners[1]),
        Some((2, 102))
    );
    assert!(!reopened.durability.failed);
}

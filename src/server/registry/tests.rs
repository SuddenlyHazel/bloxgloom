use super::super::parallel::{
    BatchId, JobKey, OwnerJob, OwnerKey, OwnerPatch, OwnerRevision, OwnerSnapshot, PatchUsage,
};
use super::super::simulation::TickId;
use super::*;
use crate::world::world_to_chunk;
use std::sync::Arc;

fn system_id(value: &str) -> SystemId {
    SystemId::new(value).unwrap()
}

fn resource(value: &str) -> ResourceId {
    ResourceId::new(value).unwrap()
}

fn descriptor(id: &str, phase: Phase) -> SystemDescriptor {
    SystemDescriptor::new(system_id(id), phase, OwnerPartition::Chunk, 1, 0)
}

fn noop_handler(job: &OwnerJob) -> Result<OwnerPatch, SystemHandlerError> {
    Ok(OwnerPatch::new(job, (), PatchUsage::default()))
}

fn register(registry: &mut SystemRegistry, descriptor: SystemDescriptor) {
    registry.register_handler(descriptor, noop_handler).unwrap();
}

fn deterministic_plan(reverse_registration: bool) -> PhasePlan {
    let shared = resource("builtin:shared");
    let derived = resource("builtin:derived");
    let unrelated = resource("builtin:unrelated");
    let alpha = system_id("builtin:alpha");
    let middle = system_id("builtin:middle");
    let mut declarations = vec![
        descriptor("builtin:alpha", Phase::Simulation)
            .write(shared.clone())
            .neighbor_radius(1),
        descriptor("builtin:middle", Phase::Simulation)
            .read(shared.clone())
            .write(derived)
            .after(alpha),
        descriptor("builtin:omega", Phase::Simulation)
            .write(shared)
            .after(middle),
        descriptor("builtin:independent", Phase::Simulation).read(unrelated),
    ];
    if reverse_registration {
        declarations.reverse();
    }

    let mut registry = SystemRegistry::new();
    for declaration in declarations {
        register(&mut registry, declaration);
    }
    registry.freeze().unwrap()
}

#[test]
fn freeze_is_registration_order_independent_and_accepts_transitive_conflict_order() {
    let forward = deterministic_plan(false);
    let reverse = deterministic_plan(true);
    for phase in Phase::ALL {
        let forward_ids: Vec<_> = forward
            .systems(phase)
            .iter()
            .map(|system| system.id().as_str())
            .collect();
        let reverse_ids: Vec<_> = reverse
            .systems(phase)
            .iter()
            .map(|system| system.id().as_str())
            .collect();
        assert_eq!(forward_ids, reverse_ids);
    }
    let simulation_waves: Vec<Vec<_>> = forward
        .waves(Phase::Simulation)
        .map(|wave| {
            wave.systems()
                .iter()
                .map(|system| system.id().as_str())
                .collect()
        })
        .collect();
    assert_eq!(
        simulation_waves,
        [
            vec!["builtin:alpha", "builtin:independent"],
            vec!["builtin:middle"],
            vec!["builtin:omega"],
        ]
    );

    let simulation = forward.systems(Phase::Simulation);
    let ids: Vec<_> = simulation
        .iter()
        .map(|system| system.id().as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "builtin:alpha",
            "builtin:independent",
            "builtin:middle",
            "builtin:omega"
        ]
    );
    assert_eq!(forward.phase_job_budget(Phase::Simulation), 4);
    assert_eq!(forward.phase_effect_budget(Phase::Simulation), 0);
    assert_eq!(simulation[0].neighbor_radius_chunks(), 1);
    assert_eq!(simulation[0].partition(), OwnerPartition::Chunk);

    let middle = simulation
        .iter()
        .find(|system| system.id().as_str() == "builtin:middle")
        .unwrap();
    assert_eq!(middle.phase(), Phase::Simulation);
    assert!(middle.reads().contains(&resource("builtin:shared")));
    assert!(middle.writes().contains(&resource("builtin:derived")));
    assert!(middle.dependencies().contains(&system_id("builtin:alpha")));
}

#[test]
fn unordered_read_write_and_write_write_conflicts_are_rejected() {
    let shared = resource("builtin:state");
    for (first, second) in [
        (
            descriptor("builtin:writer", Phase::Simulation).write(shared.clone()),
            descriptor("builtin:reader", Phase::Simulation).read(shared.clone()),
        ),
        (
            descriptor("builtin:writer-a", Phase::Simulation).write(shared.clone()),
            descriptor("builtin:writer-b", Phase::Simulation).write(shared.clone()),
        ),
    ] {
        let mut registry = SystemRegistry::new();
        registry.register(first).unwrap();
        registry.register(second).unwrap();
        assert!(matches!(
            registry.freeze(),
            Err(RegistryError::UnorderedAccessConflict { .. })
        ));
    }
}

#[test]
fn disjoint_and_read_only_accesses_can_share_a_phase() {
    let mut registry = SystemRegistry::new();
    register(
        &mut registry,
        descriptor("builtin:read-a", Phase::Simulation).read(resource("builtin:a")),
    );
    register(
        &mut registry,
        descriptor("builtin:read-b", Phase::Simulation).read(resource("builtin:a")),
    );
    register(
        &mut registry,
        descriptor("builtin:write-c", Phase::Simulation).write(resource("builtin:c")),
    );

    let plan = registry.freeze().unwrap();
    assert_eq!(plan.systems(Phase::Simulation).len(), 3);
    assert_eq!(plan.waves(Phase::Simulation).count(), 1);
}

#[test]
fn missing_later_phase_and_cyclic_dependencies_are_rejected() {
    let mut missing = SystemRegistry::new();
    missing
        .register(
            descriptor("builtin:system", Phase::Simulation).after(system_id("builtin:missing")),
        )
        .unwrap();
    assert!(matches!(
        missing.freeze(),
        Err(RegistryError::MissingDependency { .. })
    ));

    let mut later_phase = SystemRegistry::new();
    later_phase
        .register(
            descriptor("builtin:early", Phase::InputAuthorization).after(system_id("builtin:late")),
        )
        .unwrap();
    later_phase
        .register(descriptor("builtin:late", Phase::Publish))
        .unwrap();
    assert!(matches!(
        later_phase.freeze(),
        Err(RegistryError::DependencyInLaterPhase { .. })
    ));

    let mut cycle = SystemRegistry::new();
    cycle
        .register(descriptor("builtin:a", Phase::Simulation).after(system_id("builtin:b")))
        .unwrap();
    cycle
        .register(descriptor("builtin:b", Phase::Simulation).after(system_id("builtin:a")))
        .unwrap();
    assert!(matches!(
        cycle.freeze(),
        Err(RegistryError::DependencyCycle { systems }) if systems.len() == 2
    ));
}

#[test]
fn duplicate_ids_and_invalid_namespaced_ids_are_rejected() {
    let mut registry = SystemRegistry::new();
    registry
        .register(descriptor("builtin:same", Phase::Simulation))
        .unwrap();
    assert_eq!(
        registry.register(descriptor("builtin:same", Phase::Publish)),
        Err(RegistryError::DuplicateSystem {
            id: system_id("builtin:same"),
        })
    );

    assert!(matches!(
        SystemId::new("Builtin:Uppercase"),
        Err(IdentifierError::InvalidFormat { .. })
    ));
    assert!(matches!(
        ResourceId::new(format!("builtin:{}", "x".repeat(MAX_STABLE_ID_BYTES))),
        Err(IdentifierError::TooLong { .. })
    ));
}

#[test]
fn freeze_rejects_metadata_without_an_executable_handler() {
    let mut registry = SystemRegistry::new();
    registry
        .register(descriptor("builtin:metadata-only", Phase::Simulation))
        .unwrap();

    assert_eq!(
        registry.freeze().unwrap_err(),
        RegistryError::MissingHandler {
            system: system_id("builtin:metadata-only"),
        }
    );
}

#[test]
fn legacy_freeze_requires_an_exact_adapter_allowlist_and_never_prepares_it() {
    let legacy_id = system_id("builtin:legacy");
    let mut registry = SystemRegistry::new();
    registry
        .register(descriptor("builtin:legacy", Phase::Simulation))
        .unwrap();
    registry
        .register_handler(descriptor("builtin:real", Phase::Simulation), noop_handler)
        .unwrap();

    let plan = registry
        .freeze_legacy([legacy_id.clone()])
        .expect("the explicitly listed metadata adapter is permitted");
    let legacy = plan.system(&legacy_id).unwrap();
    let real_id = system_id("builtin:real");
    let real = plan.system(&real_id).unwrap();
    assert!(legacy.is_coordinator_adapter());
    assert!(!legacy.has_executable_handler());
    assert!(legacy.handler().is_none());
    assert!(!real.is_coordinator_adapter());
    assert!(real.has_executable_handler());
    assert!(real.handler().is_some());

    let owner = OwnerKey::chunk(world_to_chunk(0, 0, 0).0);
    let job = OwnerJob::new(
        legacy_id.clone(),
        JobKey::new(
            BatchId::new(TickId::new(1), Phase::Simulation, 0),
            owner,
            0,
            0,
        ),
        vec![OwnerSnapshot::new(owner, 0, Arc::new(()))],
    )
    .unwrap();
    assert!(matches!(
        legacy.prepare(&job),
        Err(SystemHandlerError::CoordinatorAdapterOnly { system }) if system == legacy_id
    ));
}

#[test]
fn legacy_freeze_rejects_unlisted_unknown_and_executable_adapter_ids() {
    let missing_id = system_id("builtin:missing");
    let mut missing = SystemRegistry::new();
    missing
        .register(descriptor("builtin:missing", Phase::Simulation))
        .unwrap();
    assert!(matches!(
        missing.freeze_legacy([]),
        Err(RegistryError::MissingHandler { system }) if system == missing_id
    ));

    let mut unknown = SystemRegistry::new();
    unknown
        .register(descriptor("builtin:present", Phase::Simulation))
        .unwrap();
    assert!(matches!(
        unknown.freeze_legacy([system_id("builtin:unknown")]),
        Err(RegistryError::UnknownLegacyAdapter { system })
            if system == system_id("builtin:unknown")
    ));

    let real_id = system_id("builtin:real");
    let mut executable = SystemRegistry::new();
    executable
        .register_handler(descriptor("builtin:real", Phase::Simulation), noop_handler)
        .unwrap();
    assert!(matches!(
        executable.freeze_legacy([real_id.clone()]),
        Err(RegistryError::LegacyAdapterHasHandler { system }) if system == real_id
    ));
}

#[test]
fn frozen_registry_dispatches_the_registered_typed_owner_handler() {
    let owner = OwnerKey::chunk(world_to_chunk(-1, 0, 0).0);
    let mut registry = SystemRegistry::new();
    registry
        .register_handler(
            descriptor("builtin:typed", Phase::Simulation),
            move |job: &OwnerJob| {
                let value = job
                    .snapshot(owner)
                    .and_then(|snapshot| snapshot.value::<u64>())
                    .copied()
                    .ok_or_else(|| SystemHandlerError::Rejected("missing typed view".into()))?;
                Ok(OwnerPatch::new(job, value + 1, PatchUsage::default()))
            },
        )
        .unwrap();
    let plan = registry.freeze().unwrap();

    let key = JobKey::new(
        BatchId::new(TickId::new(4), Phase::Simulation, 0),
        owner,
        0,
        12,
    );
    let system = system_id("builtin:typed");
    let job = OwnerJob::new(
        system.clone(),
        key,
        vec![OwnerSnapshot::new(owner, 12, Arc::new(41_u64))],
    )
    .unwrap();
    let patch = plan.system(&system).unwrap().prepare(&job).unwrap();

    assert_eq!(patch.owner(), owner);
    assert_eq!(
        patch.revisions(),
        &[OwnerRevision {
            owner,
            revision: 12,
        }]
    );
    assert_eq!(patch.payload::<u64>(), Some(&42));
}

#[test]
fn system_and_phase_budgets_radius_and_declaration_counts_are_bounded() {
    let mut no_jobs = SystemRegistry::new();
    no_jobs
        .register(SystemDescriptor::new(
            system_id("builtin:no-jobs"),
            Phase::Simulation,
            OwnerPartition::Global,
            0,
            0,
        ))
        .unwrap();
    assert!(matches!(
        no_jobs.freeze(),
        Err(RegistryError::ZeroJobBudget { .. })
    ));

    let mut too_many_jobs = SystemRegistry::new();
    too_many_jobs
        .register(SystemDescriptor::new(
            system_id("builtin:too-many-jobs"),
            Phase::Simulation,
            OwnerPartition::Global,
            MAX_SYSTEM_JOBS_PER_TICK + 1,
            0,
        ))
        .unwrap();
    assert!(matches!(
        too_many_jobs.freeze(),
        Err(RegistryError::SystemBudgetTooLarge {
            kind: BudgetKind::Jobs,
            ..
        })
    ));

    let mut too_many_effects = SystemRegistry::new();
    too_many_effects
        .register(SystemDescriptor::new(
            system_id("builtin:too-many-effects"),
            Phase::Simulation,
            OwnerPartition::Global,
            1,
            MAX_SYSTEM_EFFECTS_PER_TICK + 1,
        ))
        .unwrap();
    assert!(matches!(
        too_many_effects.freeze(),
        Err(RegistryError::SystemBudgetTooLarge {
            kind: BudgetKind::Effects,
            ..
        })
    ));

    let mut too_many_phase_jobs = SystemRegistry::new();
    for (id, jobs) in [("builtin:first", 9_000), ("builtin:second", 9_000)] {
        too_many_phase_jobs
            .register(SystemDescriptor::new(
                system_id(id),
                Phase::Simulation,
                OwnerPartition::Global,
                jobs,
                0,
            ))
            .unwrap();
    }
    assert!(matches!(
        too_many_phase_jobs.freeze(),
        Err(RegistryError::PhaseBudgetTooLarge {
            kind: BudgetKind::Jobs,
            ..
        })
    ));

    let mut too_many_domains = SystemRegistry::new();
    let mut declaration = descriptor("builtin:many-domains", Phase::Simulation);
    for index in 0..=MAX_RESOURCE_DOMAINS_PER_SYSTEM {
        declaration = declaration.read(resource(&format!("builtin:domain-{index}")));
    }
    too_many_domains.register(declaration).unwrap();
    assert!(matches!(
        too_many_domains.freeze(),
        Err(RegistryError::TooManyResourceDomains { .. })
    ));

    let mut too_many_dependencies = SystemRegistry::new();
    let mut declaration = descriptor("builtin:many-dependencies", Phase::Simulation);
    for index in 0..=MAX_DEPENDENCIES_PER_SYSTEM {
        declaration = declaration.after(system_id(&format!("builtin:dependency-{index}")));
    }
    too_many_dependencies.register(declaration).unwrap();
    assert!(matches!(
        too_many_dependencies.freeze(),
        Err(RegistryError::TooManyDependencies { .. })
    ));

    let too_large_radius =
        descriptor("builtin:wide", Phase::Simulation).neighbor_radius(MAX_NEIGHBOR_RADIUS + 1);
    let mut too_wide = SystemRegistry::new();
    too_wide.register(too_large_radius).unwrap();
    assert!(matches!(
        too_wide.freeze(),
        Err(RegistryError::NeighborRadiusTooLarge { .. })
    ));
}

#[test]
fn zero_effect_budget_is_an_explicit_no_effect_contract_and_chunk_owners_can_be_negative() {
    let owner = world_to_chunk(-1, 0, 0).0;
    assert_eq!(owner.x, -1);

    let mut registry = SystemRegistry::new();
    register(
        &mut registry,
        descriptor("builtin:negative-chunk-job", Phase::Simulation).neighbor_radius(1),
    );
    let plan = registry.freeze().unwrap();
    let scheduled = &plan.systems(Phase::Simulation)[0];
    assert_eq!(scheduled.partition(), OwnerPartition::Chunk);
    assert_eq!(scheduled.max_effects_per_tick(), 0);
    assert_eq!(plan.phase_effect_budget(Phase::Simulation), 0);
}

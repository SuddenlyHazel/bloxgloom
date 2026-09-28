//! Executable startup contracts for deterministic server systems.
//!
//! The registry freezes descriptors and their handlers together. A handler
//! receives immutable, revisioned owner views and may only return scratch data;
//! the runtime owns validation and application.

use super::parallel::{
    MAX_EFFECTS_PER_OWNER_JOB, MAX_OWNER_WAVE_PATCH_BYTES, OwnerJob, OwnerKey, OwnerPatch,
    OwnerWaveLimits,
};
use super::simulation::Phase;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io;
use std::ops::Deref;
#[cfg(test)]
use std::ops::Range;
use std::sync::Arc;

pub const MAX_REGISTERED_SYSTEMS: usize = 256;
pub const MAX_SYSTEM_JOBS_PER_TICK: usize = 16_384;
pub const MAX_SYSTEM_EFFECTS_PER_TICK: usize = 16_384;
pub const MAX_PHASE_JOBS_PER_TICK: usize = 16_384;
pub const MAX_PHASE_EFFECTS_PER_TICK: usize = 16_384;
pub const MAX_NEIGHBOR_RADIUS: u8 = 8;
pub const MAX_STABLE_ID_BYTES: usize = 128;
pub const MAX_RESOURCE_DOMAINS_PER_SYSTEM: usize = 128;
pub const MAX_DEPENDENCIES_PER_SYSTEM: usize = 64;

/// Server-trusted admission callback frozen beside a system descriptor. This
/// is deliberately not part of `SystemHandler`: mods receive immutable owner
/// jobs, while only server code can register a callback touching live state.
pub(in crate::server) type TrustedDriver =
    for<'a> fn(&mut super::runtime::CoordinatorContext<'a>) -> io::Result<()>;

/// A bounded rejection returned by a handler before its result can enter a wave.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SystemHandlerError {
    Rejected(String),
    WrongSystem {
        expected: SystemId,
        actual: SystemId,
    },
    WrongPhase {
        expected: Phase,
        actual: Phase,
    },
    WrongWave {
        expected: u16,
        actual: u16,
    },
    WrongPartition {
        expected: OwnerPartition,
        actual: OwnerKey,
    },
    CoordinatorAdapterOnly {
        system: SystemId,
    },
}

/// Runtime implementation attached to one frozen system descriptor.
///
/// Implementations are called only with immutable owner jobs. They cannot
/// access live server state through this interface and return typed scratch
/// patches for the runtime's post-validation apply step.
pub trait SystemHandler: Send + Sync + 'static {
    fn prepare(&self, job: &OwnerJob) -> Result<OwnerPatch, SystemHandlerError>;
}

impl<F> SystemHandler for F
where
    F: Fn(&OwnerJob) -> Result<OwnerPatch, SystemHandlerError> + Send + Sync + 'static,
{
    fn prepare(&self, job: &OwnerJob) -> Result<OwnerPatch, SystemHandlerError> {
        self(job)
    }
}

/// Stable, lowercase `namespace:name` identity for a server system.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SystemId(String);

impl SystemId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_namespaced(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable, lowercase `namespace:name` identity for a declared resource domain.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceId(String);

impl ResourceId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_namespaced(&value)?;
        Ok(Self(value))
    }
}

fn validate_namespaced(value: &str) -> Result<(), IdentifierError> {
    if value.len() > MAX_STABLE_ID_BYTES {
        return Err(IdentifierError::TooLong {
            value: value.into(),
            maximum_bytes: MAX_STABLE_ID_BYTES,
        });
    }
    let valid_component = |component: &str| {
        !component.is_empty()
            && component.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
            })
    };
    let mut parts = value.split(':');
    let valid = parts.next().is_some_and(valid_component)
        && parts.next().is_some_and(valid_component)
        && parts.next().is_none();
    if valid {
        Ok(())
    } else {
        Err(IdentifierError::InvalidFormat {
            value: value.into(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentifierError {
    InvalidFormat { value: String },
    TooLong { value: String, maximum_bytes: usize },
}

/// Ownership key used to partition a system's candidate jobs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OwnerPartition {
    /// Per-chunk ownership is used by extension terrain and block systems.
    #[allow(dead_code)]
    Chunk,
    Entity,
    /// Reserved for profile-local systems; today's inventory transactions
    /// also touch chunks and drops, so their actual batch is global.
    #[allow(dead_code)]
    Profile,
    Global,
}

/// Startup declaration for one immutable/read-only system job family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemDescriptor {
    id: SystemId,
    phase: Phase,
    partition: OwnerPartition,
    reads: BTreeSet<ResourceId>,
    writes: BTreeSet<ResourceId>,
    after: BTreeSet<SystemId>,
    neighbor_radius: u8,
    world_read_radius: Option<u8>,
    max_jobs_per_tick: usize,
    /// Zero is valid and means this system is not permitted to emit effects.
    max_effects_per_tick: usize,
    max_effects_per_job: usize,
}

impl SystemDescriptor {
    pub fn new(
        id: SystemId,
        phase: Phase,
        partition: OwnerPartition,
        max_jobs_per_tick: usize,
        max_effects_per_tick: usize,
    ) -> Self {
        Self {
            id,
            phase,
            partition,
            reads: BTreeSet::new(),
            writes: BTreeSet::new(),
            after: BTreeSet::new(),
            neighbor_radius: 0,
            world_read_radius: None,
            max_jobs_per_tick,
            max_effects_per_tick,
            max_effects_per_job: max_effects_per_tick.min(MAX_EFFECTS_PER_OWNER_JOB),
        }
    }

    pub fn read(mut self, resource: ResourceId) -> Self {
        self.reads.insert(resource);
        self
    }

    pub fn write(mut self, resource: ResourceId) -> Self {
        self.writes.insert(resource);
        self
    }

    /// Limits effects emitted by any one owner job. The constructor defaults
    /// this to the system bound capped at the initial per-producer ceiling.
    pub const fn effects_per_job(mut self, maximum: usize) -> Self {
        self.max_effects_per_job = maximum;
        self
    }

    pub fn after(mut self, dependency: SystemId) -> Self {
        self.after.insert(dependency);
        self
    }

    /// Declares neighboring owner-state snapshots, not terrain reads. Public
    /// systems use `read_chunks` for authoritative terrain capture instead.
    #[allow(dead_code)]
    pub const fn neighbor_radius(mut self, radius: u8) -> Self {
        self.neighbor_radius = radius;
        self
    }

    pub(in crate::server) const fn read_chunks(mut self, radius: u8) -> Self {
        self.world_read_radius = Some(radius);
        self
    }

    pub(in crate::server) const fn world_read_radius(&self) -> Option<u8> {
        self.world_read_radius
    }

    pub fn id(&self) -> &SystemId {
        &self.id
    }

    /// Phase metadata for extension tooling that inspects a frozen plan.
    #[allow(dead_code)]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// Owner partition metadata for extension tooling that inspects a plan.
    #[allow(dead_code)]
    pub const fn partition(&self) -> OwnerPartition {
        self.partition
    }

    /// Read domains declared by this system, for scheduler and tooling checks.
    #[allow(dead_code)]
    pub fn reads(&self) -> &BTreeSet<ResourceId> {
        &self.reads
    }

    /// Write domains declared by this system, for scheduler and tooling checks.
    #[allow(dead_code)]
    pub fn writes(&self) -> &BTreeSet<ResourceId> {
        &self.writes
    }

    /// Explicit prerequisites declared by this system.
    #[allow(dead_code)]
    pub fn dependencies(&self) -> &BTreeSet<SystemId> {
        &self.after
    }

    /// Neighbor radius declared for chunk-scoped extension work.
    #[allow(dead_code)]
    pub const fn neighbor_radius_chunks(&self) -> u8 {
        self.neighbor_radius
    }

    /// Per-system job bound used by frozen phase budget calculations.
    #[allow(dead_code)]
    pub const fn max_jobs_per_tick(&self) -> usize {
        self.max_jobs_per_tick
    }

    /// Per-system effect bound used by frozen phase budget calculations.
    #[allow(dead_code)]
    pub const fn max_effects_per_tick(&self) -> usize {
        self.max_effects_per_tick
    }

    pub const fn max_effects_per_job(&self) -> usize {
        self.max_effects_per_job
    }

    pub fn accepts_owner(&self, owner: OwnerKey) -> bool {
        match self.partition {
            OwnerPartition::Chunk => matches!(owner, OwnerKey::Chunk(_)),
            OwnerPartition::Entity => matches!(owner, OwnerKey::Entity(_)),
            OwnerPartition::Profile => matches!(owner, OwnerKey::Profile(_)),
            OwnerPartition::Global => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetKind {
    Jobs,
    Effects,
    EffectsPerJob,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    TooManySystems {
        maximum: usize,
    },
    TooManyResourceDomains {
        system: SystemId,
        requested: usize,
        maximum: usize,
    },
    TooManyDependencies {
        system: SystemId,
        requested: usize,
        maximum: usize,
    },
    DuplicateSystem {
        id: SystemId,
    },
    MissingHandler {
        system: SystemId,
    },
    UnknownLegacyAdapter {
        system: SystemId,
    },
    LegacyAdapterHasHandler {
        system: SystemId,
    },
    ZeroJobBudget {
        system: SystemId,
    },
    SystemBudgetTooLarge {
        system: SystemId,
        kind: BudgetKind,
        requested: usize,
        maximum: usize,
    },
    PhaseBudgetTooLarge {
        phase: Phase,
        kind: BudgetKind,
        requested: usize,
        maximum: usize,
    },
    NeighborRadiusTooLarge {
        system: SystemId,
        requested: u8,
        maximum: u8,
    },
    MissingDependency {
        system: SystemId,
        dependency: SystemId,
    },
    DependencyInLaterPhase {
        system: SystemId,
        dependency: SystemId,
        system_phase: Phase,
        dependency_phase: Phase,
    },
    DependencyCycle {
        /// Sorted IDs in the unresolved dependency subgraph.
        systems: Vec<SystemId>,
    },
    UnorderedAccessConflict {
        phase: Phase,
        first: SystemId,
        second: SystemId,
        resource: ResourceId,
        first_access: AccessKind,
        second_access: AccessKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessKind {
    Read,
    Write,
}

struct PendingSystem {
    descriptor: SystemDescriptor,
    handler: Option<Arc<dyn SystemHandler>>,
    driver: Option<TrustedDriver>,
}

#[derive(Clone)]
enum RegisteredHandler {
    Executable(Arc<dyn SystemHandler>),
    /// Explicit transitional mode for a coordinator-owned implementation.
    /// `prepare` rejects this mode; it is not a placeholder no-op handler.
    CoordinatorAdapter,
}

/// A registered descriptor and its executable implementation.
#[derive(Clone)]
pub struct ExecutableSystem {
    descriptor: SystemDescriptor,
    handler: RegisteredHandler,
    driver: Option<TrustedDriver>,
    wave_index: u16,
}

impl ExecutableSystem {
    pub(in crate::server) const fn driver(&self) -> Option<TrustedDriver> {
        self.driver
    }

    #[cfg(test)]
    pub fn handler(&self) -> Option<&dyn SystemHandler> {
        match &self.handler {
            RegisteredHandler::Executable(handler) => Some(handler.as_ref()),
            RegisteredHandler::CoordinatorAdapter => None,
        }
    }

    pub const fn has_executable_handler(&self) -> bool {
        matches!(&self.handler, RegisteredHandler::Executable(_))
    }

    pub const fn is_coordinator_adapter(&self) -> bool {
        matches!(&self.handler, RegisteredHandler::CoordinatorAdapter)
    }

    pub fn prepare(&self, job: &OwnerJob) -> Result<OwnerPatch, SystemHandlerError> {
        self.prepare_with_wave(job, self.wave_index)
    }

    /// Runs a handler from the live server executor. Its batch wave is a
    /// unique dispatch ordinal within the phase (several systems may share one
    /// logical dependency wave), so registration, phase, and ownership are
    /// checked here while the registry's frozen order supplies dependency
    /// ordering.
    pub(in crate::server) fn prepare_in_dispatch(
        &self,
        job: &OwnerJob,
        dispatch_wave: u16,
    ) -> Result<OwnerPatch, SystemHandlerError> {
        self.prepare_with_wave(job, dispatch_wave)
    }

    fn prepare_with_wave(
        &self,
        job: &OwnerJob,
        expected_wave: u16,
    ) -> Result<OwnerPatch, SystemHandlerError> {
        if job.system() != self.id() {
            return Err(SystemHandlerError::WrongSystem {
                expected: self.id().clone(),
                actual: job.system().clone(),
            });
        }
        if job.key().batch.phase() != self.phase() {
            return Err(SystemHandlerError::WrongPhase {
                expected: self.phase(),
                actual: job.key().batch.phase(),
            });
        }
        if job.key().batch.wave() != expected_wave {
            return Err(SystemHandlerError::WrongWave {
                expected: expected_wave,
                actual: job.key().batch.wave(),
            });
        }
        if !self.accepts_owner(job.owner()) {
            return Err(SystemHandlerError::WrongPartition {
                expected: self.partition(),
                actual: job.owner(),
            });
        }
        match &self.handler {
            RegisteredHandler::Executable(handler) => handler.prepare(job),
            RegisteredHandler::CoordinatorAdapter => {
                Err(SystemHandlerError::CoordinatorAdapterOnly {
                    system: self.id().clone(),
                })
            }
        }
    }

    pub const fn wave_index(&self) -> u16 {
        self.wave_index
    }
}

impl Deref for ExecutableSystem {
    type Target = SystemDescriptor;

    fn deref(&self) -> &Self::Target {
        &self.descriptor
    }
}

impl fmt::Debug for ExecutableSystem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutableSystem")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

/// Mutable startup builder. Every descriptor must have a handler before it
/// can be frozen into the deterministic tick plan.
#[derive(Default)]
pub struct SystemRegistry {
    systems: BTreeMap<SystemId, PendingSystem>,
}

impl SystemRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    // Metadata-only fixtures exercise missing-handler and legacy-adapter validation.
    #[cfg(test)]
    pub fn register(&mut self, descriptor: SystemDescriptor) -> Result<(), RegistryError> {
        self.insert(descriptor, None, None)
    }

    /// Registers an executable handler with its descriptor.
    #[cfg(test)]
    pub fn register_handler<H: SystemHandler>(
        &mut self,
        descriptor: SystemDescriptor,
        handler: H,
    ) -> Result<(), RegistryError> {
        self.insert(descriptor, Some(Arc::new(handler)), None)
    }

    /// Registers an already shared handler, useful when the same immutable
    /// implementation is configured for more than one system identity.
    pub fn register_shared_handler(
        &mut self,
        descriptor: SystemDescriptor,
        handler: Arc<dyn SystemHandler>,
    ) -> Result<(), RegistryError> {
        self.insert(descriptor, Some(handler), None)
    }

    /// Registers an immutable worker handler with its trusted admission
    /// driver. The driver only selects owners and stages validated output;
    /// gameplay candidate computation remains in `SystemHandler::prepare`.
    pub(in crate::server) fn register_handler_with_driver<H: SystemHandler>(
        &mut self,
        descriptor: SystemDescriptor,
        handler: H,
        driver: TrustedDriver,
    ) -> Result<(), RegistryError> {
        self.insert(descriptor, Some(Arc::new(handler)), Some(driver))
    }

    /// Registers a trusted transitional coordinator implementation. Unlike
    /// mod-facing handlers, this callback can touch live `State` and is only
    /// callable from the fixed-step server runtime.
    pub(in crate::server) fn register_coordinator_adapter(
        &mut self,
        descriptor: SystemDescriptor,
        driver: TrustedDriver,
    ) -> Result<(), RegistryError> {
        self.insert(descriptor, None, Some(driver))
    }

    fn insert(
        &mut self,
        descriptor: SystemDescriptor,
        handler: Option<Arc<dyn SystemHandler>>,
        driver: Option<TrustedDriver>,
    ) -> Result<(), RegistryError> {
        if self.systems.contains_key(&descriptor.id) {
            return Err(RegistryError::DuplicateSystem { id: descriptor.id });
        }
        if self.systems.len() >= MAX_REGISTERED_SYSTEMS {
            return Err(RegistryError::TooManySystems {
                maximum: MAX_REGISTERED_SYSTEMS,
            });
        }
        self.systems.insert(
            descriptor.id.clone(),
            PendingSystem {
                descriptor,
                handler,
                driver,
            },
        );
        Ok(())
    }

    pub fn freeze(self) -> Result<PhasePlan, RegistryError> {
        self.freeze_inner(BTreeSet::new())
    }

    /// Legacy metadata-only fixture entrypoint. Live adapters register their
    /// trusted driver explicitly with `register_coordinator_adapter`.
    #[cfg(test)]
    pub fn freeze_legacy(
        self,
        coordinator_adapters: impl IntoIterator<Item = SystemId>,
    ) -> Result<PhasePlan, RegistryError> {
        self.freeze_inner(coordinator_adapters.into_iter().collect())
    }

    fn freeze_inner(
        self,
        coordinator_adapters: BTreeSet<SystemId>,
    ) -> Result<PhasePlan, RegistryError> {
        self.validate_budgets()?;
        let dependencies = self.validate_dependencies()?;
        let ordered_phases = Phase::ALL
            .into_iter()
            .map(|phase| self.sorted_phase(phase, &dependencies))
            .collect::<Result<Vec<_>, _>>()?;

        self.validate_conflicts(&dependencies)?;
        self.validate_handlers(&coordinator_adapters)?;
        let mut phases = Vec::with_capacity(Phase::ALL.len());
        let mut system_indexes = BTreeMap::new();
        for ordered_waves in ordered_phases {
            let phase_index = phases.len();
            let mut systems = Vec::new();
            #[cfg(test)]
            let mut wave_ranges = Vec::with_capacity(ordered_waves.len());
            for (wave_index, wave) in ordered_waves.into_iter().enumerate() {
                #[cfg(test)]
                let start = systems.len();
                for id in wave {
                    let system_index = systems.len();
                    let pending = self
                        .systems
                        .get(&id)
                        .expect("sorted system came from registry");
                    systems.push(ExecutableSystem {
                        descriptor: pending.descriptor.clone(),
                        handler: match &pending.handler {
                            Some(handler) => RegisteredHandler::Executable(Arc::clone(handler)),
                            None => RegisteredHandler::CoordinatorAdapter,
                        },
                        driver: pending.driver,
                        wave_index: u16::try_from(wave_index)
                            .expect("system count bounds phase wave count"),
                    });
                    system_indexes.insert(id, (phase_index, system_index));
                }
                #[cfg(test)]
                wave_ranges.push(start..systems.len());
            }
            phases.push(PhaseSchedule {
                systems,
                #[cfg(test)]
                wave_ranges,
            });
        }
        Ok(PhasePlan {
            phases,
            system_indexes,
        })
    }

    fn validate_handlers(
        &self,
        coordinator_adapters: &BTreeSet<SystemId>,
    ) -> Result<(), RegistryError> {
        for id in coordinator_adapters {
            let Some(pending) = self.systems.get(id) else {
                return Err(RegistryError::UnknownLegacyAdapter { system: id.clone() });
            };
            if pending.handler.is_some() || pending.driver.is_some() {
                return Err(RegistryError::LegacyAdapterHasHandler { system: id.clone() });
            }
        }
        for (id, pending) in &self.systems {
            if pending.handler.is_none()
                && pending.driver.is_none()
                && !coordinator_adapters.contains(id)
            {
                return Err(RegistryError::MissingHandler { system: id.clone() });
            }
        }
        Ok(())
    }

    fn validate_budgets(&self) -> Result<(), RegistryError> {
        let mut phase_jobs = BTreeMap::<Phase, usize>::new();
        let mut phase_effects = BTreeMap::<Phase, usize>::new();
        for pending in self.systems.values() {
            let descriptor = &pending.descriptor;
            let access_domains = descriptor.reads.union(&descriptor.writes).count();
            if access_domains > MAX_RESOURCE_DOMAINS_PER_SYSTEM {
                return Err(RegistryError::TooManyResourceDomains {
                    system: descriptor.id.clone(),
                    requested: access_domains,
                    maximum: MAX_RESOURCE_DOMAINS_PER_SYSTEM,
                });
            }
            if descriptor.after.len() > MAX_DEPENDENCIES_PER_SYSTEM {
                return Err(RegistryError::TooManyDependencies {
                    system: descriptor.id.clone(),
                    requested: descriptor.after.len(),
                    maximum: MAX_DEPENDENCIES_PER_SYSTEM,
                });
            }
            if descriptor.max_jobs_per_tick == 0 {
                return Err(RegistryError::ZeroJobBudget {
                    system: descriptor.id.clone(),
                });
            }
            if descriptor.max_jobs_per_tick > MAX_SYSTEM_JOBS_PER_TICK {
                return Err(RegistryError::SystemBudgetTooLarge {
                    system: descriptor.id.clone(),
                    kind: BudgetKind::Jobs,
                    requested: descriptor.max_jobs_per_tick,
                    maximum: MAX_SYSTEM_JOBS_PER_TICK,
                });
            }
            if descriptor.max_effects_per_tick > MAX_SYSTEM_EFFECTS_PER_TICK {
                return Err(RegistryError::SystemBudgetTooLarge {
                    system: descriptor.id.clone(),
                    kind: BudgetKind::Effects,
                    requested: descriptor.max_effects_per_tick,
                    maximum: MAX_SYSTEM_EFFECTS_PER_TICK,
                });
            }
            if descriptor.max_effects_per_job > MAX_EFFECTS_PER_OWNER_JOB
                || descriptor.max_effects_per_job > descriptor.max_effects_per_tick
            {
                return Err(RegistryError::SystemBudgetTooLarge {
                    system: descriptor.id.clone(),
                    kind: BudgetKind::EffectsPerJob,
                    requested: descriptor.max_effects_per_job,
                    maximum: MAX_EFFECTS_PER_OWNER_JOB.min(descriptor.max_effects_per_tick),
                });
            }
            if descriptor.neighbor_radius > MAX_NEIGHBOR_RADIUS {
                return Err(RegistryError::NeighborRadiusTooLarge {
                    system: descriptor.id.clone(),
                    requested: descriptor.neighbor_radius,
                    maximum: MAX_NEIGHBOR_RADIUS,
                });
            }

            let jobs = phase_jobs.entry(descriptor.phase).or_default();
            *jobs = jobs.saturating_add(descriptor.max_jobs_per_tick);
            if *jobs > MAX_PHASE_JOBS_PER_TICK {
                return Err(RegistryError::PhaseBudgetTooLarge {
                    phase: descriptor.phase,
                    kind: BudgetKind::Jobs,
                    requested: *jobs,
                    maximum: MAX_PHASE_JOBS_PER_TICK,
                });
            }
            let effects = phase_effects.entry(descriptor.phase).or_default();
            *effects = effects.saturating_add(descriptor.max_effects_per_tick);
            if *effects > MAX_PHASE_EFFECTS_PER_TICK {
                return Err(RegistryError::PhaseBudgetTooLarge {
                    phase: descriptor.phase,
                    kind: BudgetKind::Effects,
                    requested: *effects,
                    maximum: MAX_PHASE_EFFECTS_PER_TICK,
                });
            }
        }
        Ok(())
    }

    fn validate_dependencies(
        &self,
    ) -> Result<BTreeMap<SystemId, BTreeSet<SystemId>>, RegistryError> {
        let mut dependencies = BTreeMap::new();
        for pending in self.systems.values() {
            let descriptor = &pending.descriptor;
            for dependency in &descriptor.after {
                let Some(dependency_system) = self.systems.get(dependency) else {
                    return Err(RegistryError::MissingDependency {
                        system: descriptor.id.clone(),
                        dependency: dependency.clone(),
                    });
                };
                let dependency_descriptor = &dependency_system.descriptor;
                if dependency_descriptor.phase > descriptor.phase {
                    return Err(RegistryError::DependencyInLaterPhase {
                        system: descriptor.id.clone(),
                        dependency: dependency.clone(),
                        system_phase: descriptor.phase,
                        dependency_phase: dependency_descriptor.phase,
                    });
                }
            }
            dependencies.insert(descriptor.id.clone(), descriptor.after.clone());
        }
        Ok(dependencies)
    }

    fn sorted_phase(
        &self,
        phase: Phase,
        dependencies: &BTreeMap<SystemId, BTreeSet<SystemId>>,
    ) -> Result<Vec<Vec<SystemId>>, RegistryError> {
        let mut remaining: BTreeSet<SystemId> = self
            .systems
            .values()
            .filter(|pending| pending.descriptor.phase == phase)
            .map(|pending| pending.descriptor.id.clone())
            .collect();
        let mut waves = Vec::new();

        while !remaining.is_empty() {
            let ready: Vec<_> = remaining
                .iter()
                .filter(|system| {
                    dependencies[*system].iter().all(|dependency| {
                        self.systems[dependency].descriptor.phase < phase
                            || !remaining.contains(dependency)
                    })
                })
                .cloned()
                .collect();
            if ready.is_empty() {
                return Err(RegistryError::DependencyCycle {
                    systems: remaining.into_iter().collect(),
                });
            }
            for system in &ready {
                remaining.remove(system);
            }
            waves.push(ready);
        }
        Ok(waves)
    }

    fn validate_conflicts(
        &self,
        dependencies: &BTreeMap<SystemId, BTreeSet<SystemId>>,
    ) -> Result<(), RegistryError> {
        for phase in Phase::ALL {
            let systems: Vec<_> = self
                .systems
                .values()
                .filter(|pending| pending.descriptor.phase == phase)
                .map(|pending| &pending.descriptor)
                .collect();
            for (index, first) in systems.iter().enumerate() {
                for second in systems.iter().skip(index + 1) {
                    if depends_on(dependencies, &second.id, &first.id)
                        || depends_on(dependencies, &first.id, &second.id)
                    {
                        continue;
                    }
                    if let Some(resource) = first
                        .writes
                        .iter()
                        .chain(first.reads.iter())
                        .filter(|resource| {
                            (first.writes.contains(*resource)
                                && (second.reads.contains(*resource)
                                    || second.writes.contains(*resource)))
                                || (first.reads.contains(*resource)
                                    && second.writes.contains(*resource))
                        })
                        .min()
                    {
                        return Err(RegistryError::UnorderedAccessConflict {
                            phase,
                            first: first.id.clone(),
                            second: second.id.clone(),
                            resource: resource.clone(),
                            first_access: access(first, resource),
                            second_access: access(second, resource),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

fn access(descriptor: &SystemDescriptor, resource: &ResourceId) -> AccessKind {
    if descriptor.writes.contains(resource) {
        AccessKind::Write
    } else {
        AccessKind::Read
    }
}

/// True if `system` has a direct or transitive `after` dependency on `ancestor`.
fn depends_on(
    dependencies: &BTreeMap<SystemId, BTreeSet<SystemId>>,
    system: &SystemId,
    ancestor: &SystemId,
) -> bool {
    let mut pending = vec![system.clone()];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        for dependency in &dependencies[&current] {
            if dependency == ancestor {
                return true;
            }
            pending.push(dependency.clone());
        }
    }
    false
}

/// Immutable deterministic schedule. One coordinator may use this plan with a
/// shared fixed worker pool; registration never constructs per-system threads.
#[derive(Clone, Debug)]
pub struct PhasePlan {
    phases: Vec<PhaseSchedule>,
    system_indexes: BTreeMap<SystemId, (usize, usize)>,
}

#[derive(Clone, Debug)]
struct PhaseSchedule {
    systems: Vec<ExecutableSystem>,
    #[cfg(test)]
    wave_ranges: Vec<Range<usize>>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub struct SystemWave<'a> {
    systems: &'a [ExecutableSystem],
}

#[cfg(test)]
impl<'a> SystemWave<'a> {
    pub const fn systems(self) -> &'a [ExecutableSystem] {
        self.systems
    }
}

impl PhasePlan {
    pub fn systems(&self, phase: Phase) -> &[ExecutableSystem] {
        &self.phases[phase_index(phase)].systems
    }

    /// Dependency waves are stable and contain only systems that can run
    /// concurrently under the frozen access declarations.
    #[cfg(test)]
    pub fn waves(&self, phase: Phase) -> impl Iterator<Item = SystemWave<'_>> {
        let schedule = &self.phases[phase_index(phase)];
        schedule.wave_ranges.iter().map(move |range| SystemWave {
            systems: &schedule.systems[range.clone()],
        })
    }

    pub fn system(&self, id: &SystemId) -> Option<&ExecutableSystem> {
        let (phase, index) = self.system_indexes.get(id).copied()?;
        self.phases.get(phase)?.systems.get(index)
    }

    pub fn owner_wave_limits(&self, id: &SystemId) -> Option<OwnerWaveLimits> {
        self.system(id).map(|system| OwnerWaveLimits {
            max_jobs: system.max_jobs_per_tick,
            max_effects_per_job: system.max_effects_per_job,
            max_effect_deliveries: system.max_effects_per_tick,
            ..OwnerWaveLimits::new(
                system.max_jobs_per_tick,
                system.max_effects_per_tick,
                MAX_OWNER_WAVE_PATCH_BYTES,
            )
        })
    }

    /// Aggregate bound exposed for runtime admission control. Built-in
    /// admission is currently fixed by its registered queue limits.
    #[allow(dead_code)]
    pub fn phase_job_budget(&self, phase: Phase) -> usize {
        self.systems(phase)
            .iter()
            .map(|system| system.max_jobs_per_tick())
            .sum()
    }

    /// Aggregate effect bound exposed for runtime admission control. Built-in
    /// effect admission is currently fixed by its dispatcher limits.
    #[allow(dead_code)]
    pub fn phase_effect_budget(&self, phase: Phase) -> usize {
        self.systems(phase)
            .iter()
            .map(|system| system.max_effects_per_tick())
            .sum()
    }
}

fn phase_index(phase: Phase) -> usize {
    Phase::ALL
        .iter()
        .position(|candidate| *candidate == phase)
        .expect("every phase is listed in Phase::ALL")
}

#[cfg(test)]
#[path = "registry/tests.rs"]
mod tests;

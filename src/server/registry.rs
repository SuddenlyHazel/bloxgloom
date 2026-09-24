//! Frozen startup contracts for deterministic server systems.
//!
//! The registry describes phase ordering and bounded access; it does not own
//! an executor and never gives systems access to mutable world state.

use super::simulation::Phase;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_REGISTERED_SYSTEMS: usize = 256;
pub const MAX_SYSTEM_JOBS_PER_TICK: usize = 16_384;
pub const MAX_SYSTEM_EFFECTS_PER_TICK: usize = 16_384;
pub const MAX_PHASE_JOBS_PER_TICK: usize = 16_384;
pub const MAX_PHASE_EFFECTS_PER_TICK: usize = 16_384;
pub const MAX_NEIGHBOR_RADIUS: u8 = 8;
pub const MAX_STABLE_ID_BYTES: usize = 128;
pub const MAX_RESOURCE_DOMAINS_PER_SYSTEM: usize = 128;
pub const MAX_DEPENDENCIES_PER_SYSTEM: usize = 64;

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
    Chunk,
    Entity,
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
    max_jobs_per_tick: usize,
    /// Zero is valid and means this system is not permitted to emit effects.
    max_effects_per_tick: usize,
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
            max_jobs_per_tick,
            max_effects_per_tick,
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

    pub fn after(mut self, dependency: SystemId) -> Self {
        self.after.insert(dependency);
        self
    }

    pub const fn neighbor_radius(mut self, radius: u8) -> Self {
        self.neighbor_radius = radius;
        self
    }

    pub fn id(&self) -> &SystemId {
        &self.id
    }

    pub const fn phase(&self) -> Phase {
        self.phase
    }

    pub const fn partition(&self) -> OwnerPartition {
        self.partition
    }

    pub fn reads(&self) -> &BTreeSet<ResourceId> {
        &self.reads
    }

    pub fn writes(&self) -> &BTreeSet<ResourceId> {
        &self.writes
    }

    pub fn dependencies(&self) -> &BTreeSet<SystemId> {
        &self.after
    }

    pub const fn neighbor_radius_chunks(&self) -> u8 {
        self.neighbor_radius
    }

    pub const fn max_jobs_per_tick(&self) -> usize {
        self.max_jobs_per_tick
    }

    pub const fn max_effects_per_tick(&self) -> usize {
        self.max_effects_per_tick
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetKind {
    Jobs,
    Effects,
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

/// Mutable builder used during startup. `freeze` is the only way to obtain the
/// deterministic, immutable plan used by the tick coordinator.
#[derive(Default)]
pub struct SystemRegistry {
    systems: BTreeMap<SystemId, SystemDescriptor>,
}

impl SystemRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, descriptor: SystemDescriptor) -> Result<(), RegistryError> {
        if self.systems.contains_key(&descriptor.id) {
            return Err(RegistryError::DuplicateSystem { id: descriptor.id });
        }
        if self.systems.len() >= MAX_REGISTERED_SYSTEMS {
            return Err(RegistryError::TooManySystems {
                maximum: MAX_REGISTERED_SYSTEMS,
            });
        }
        self.systems.insert(descriptor.id.clone(), descriptor);
        Ok(())
    }

    pub fn freeze(self) -> Result<PhasePlan, RegistryError> {
        self.validate_budgets()?;
        let dependencies = self.validate_dependencies()?;

        let mut phases = Vec::with_capacity(Phase::ALL.len());
        for phase in Phase::ALL {
            phases.push(self.sorted_phase(phase, &dependencies)?);
        }
        self.validate_conflicts(&dependencies)?;
        Ok(PhasePlan { phases })
    }

    fn validate_budgets(&self) -> Result<(), RegistryError> {
        let mut phase_jobs = BTreeMap::<Phase, usize>::new();
        let mut phase_effects = BTreeMap::<Phase, usize>::new();
        for descriptor in self.systems.values() {
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
        for descriptor in self.systems.values() {
            for dependency in &descriptor.after {
                let Some(dependency_descriptor) = self.systems.get(dependency) else {
                    return Err(RegistryError::MissingDependency {
                        system: descriptor.id.clone(),
                        dependency: dependency.clone(),
                    });
                };
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
    ) -> Result<Vec<SystemDescriptor>, RegistryError> {
        let mut remaining: BTreeSet<SystemId> = self
            .systems
            .values()
            .filter(|descriptor| descriptor.phase == phase)
            .map(|descriptor| descriptor.id.clone())
            .collect();
        let mut ordered = Vec::with_capacity(remaining.len());

        while !remaining.is_empty() {
            let ready = remaining.iter().find(|system| {
                dependencies[*system].iter().all(|dependency| {
                    self.systems[dependency].phase < phase || !remaining.contains(dependency)
                })
            });
            let Some(system) = ready.cloned() else {
                return Err(RegistryError::DependencyCycle {
                    systems: remaining.into_iter().collect(),
                });
            };
            remaining.remove(&system);
            ordered.push(self.systems[&system].clone());
        }
        Ok(ordered)
    }

    fn validate_conflicts(
        &self,
        dependencies: &BTreeMap<SystemId, BTreeSet<SystemId>>,
    ) -> Result<(), RegistryError> {
        for phase in Phase::ALL {
            let systems: Vec<_> = self
                .systems
                .values()
                .filter(|descriptor| descriptor.phase == phase)
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhasePlan {
    phases: Vec<Vec<SystemDescriptor>>,
}

impl PhasePlan {
    pub fn systems(&self, phase: Phase) -> &[SystemDescriptor] {
        &self.phases[phase_index(phase)]
    }

    pub fn phase_job_budget(&self, phase: Phase) -> usize {
        self.systems(phase)
            .iter()
            .map(SystemDescriptor::max_jobs_per_tick)
            .sum()
    }

    pub fn phase_effect_budget(&self, phase: Phase) -> usize {
        self.systems(phase)
            .iter()
            .map(SystemDescriptor::max_effects_per_tick)
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

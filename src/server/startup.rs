//! Registration boundary between a world's content map and live server code.
//!
//! Built-ins and startup extensions freeze together before storage opens.
//! This is an internal trusted-native hook, not public mod loading or a
//! durable owner-state contract.

use super::builtins;
use super::effects::{EffectKindRegistry, EffectKindRegistryFrozen};
use super::entities::{
    EntityError, EntityInteractionPolicy, EntityOwnership, EntityPayloadCodec, EntityTickPolicy,
    EntityTransferPolicy, EntityTypeRegistration, EntityTypeRegistry, EntityTypeRegistryBuilder,
    TickPolicy,
};
use super::parallel::{OwnerData, OwnerKey};
use super::registry::{
    OwnerPartition, PhasePlan, SystemDescriptor, SystemHandler, SystemId, SystemRegistry,
};
use super::runtime::systems::{MAX_OWNER_VALUES_PER_SYSTEM, SystemRuntime};
use crate::content::Catalog;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};
use std::sync::Arc;

/// One stable entity implementation, resolved by canonical content key each
/// time the world's numeric assignment is loaded. Optional policies stay
/// paired with the same codec and ownership declaration.
pub(crate) struct StartupEntityType {
    pub(crate) key: String,
    pub(crate) ownership: EntityOwnership,
    pub(crate) tick_policy: TickPolicy,
    pub(crate) max_payload_bytes: usize,
    pub(crate) codec: Arc<dyn EntityPayloadCodec>,
    pub(crate) interaction_policy: Option<Arc<dyn EntityInteractionPolicy>>,
    pub(crate) tick_planner: Option<Arc<dyn EntityTickPolicy>>,
}

pub(crate) struct ServerStartup {
    catalog: Arc<Catalog>,
    entity_types: Vec<StartupEntityType>,
    transfer_policies: Vec<(String, Arc<dyn EntityTransferPolicy>)>,
    systems: Vec<(SystemDescriptor, Arc<dyn SystemHandler>)>,
    owners: Vec<(SystemId, OwnerKey, OwnerData)>,
}

impl ServerStartup {
    pub(crate) fn new(catalog: Arc<Catalog>) -> Self {
        Self {
            catalog,
            entity_types: Vec::new(),
            transfer_policies: Vec::new(),
            systems: Vec::new(),
            owners: Vec::new(),
        }
    }

    pub(crate) fn register_entity_type(&mut self, registration: StartupEntityType) {
        self.entity_types.push(registration);
    }

    /// Registers the pure exchange hooks for one entity type by content key.
    /// Kept separate from [`StartupEntityType`] so existing registrations are
    /// untouched: a passive store needs no schedule to be a transfer
    /// endpoint, and ticking types opt in independently of their planner.
    pub(crate) fn register_entity_transfer_policy(
        &mut self,
        key: String,
        policy: Arc<dyn EntityTransferPolicy>,
    ) {
        self.transfer_policies.push((key, policy));
    }

    pub(crate) fn register_system<H: SystemHandler>(
        &mut self,
        descriptor: SystemDescriptor,
        handler: H,
    ) {
        self.systems.push((descriptor, Arc::new(handler)));
    }

    pub(crate) fn seed_owner<T: Any + Send + Sync>(
        &mut self,
        system: SystemId,
        owner: OwnerKey,
        value: T,
    ) {
        self.owners.push((system, owner, OwnerData::new(value)));
    }

    pub(super) fn catalog(&self) -> Arc<Catalog> {
        Arc::clone(&self.catalog)
    }

    pub(super) fn entity_types_for(
        &self,
        catalog: Arc<Catalog>,
    ) -> io::Result<Arc<EntityTypeRegistry>> {
        let mut types = EntityTypeRegistryBuilder::new(&catalog);
        super::drops::register_entity_type(&mut types, Arc::clone(&catalog))
            .map_err(entity_error)?;
        super::entities::register_player_entity_type(&mut types).map_err(entity_error)?;
        super::entities::register_kiln_entity_type(&mut types, Arc::clone(&catalog))
            .map_err(entity_error)?;
        for registration in &self.entity_types {
            let id = catalog
                .entity_type_id_by_key(&registration.key)
                .ok_or_else(|| entity_error(EntityError::InvalidType))?;
            types
                .register(EntityTypeRegistration {
                    id,
                    ownership: registration.ownership.clone(),
                    tick_policy: registration.tick_policy,
                    max_payload_bytes: registration.max_payload_bytes,
                    codec: Arc::clone(&registration.codec),
                })
                .map_err(entity_error)?;
            if let Some(policy) = &registration.interaction_policy {
                types
                    .register_interaction_policy(id, Arc::clone(policy))
                    .map_err(entity_error)?;
            }
            if let Some(planner) = &registration.tick_planner {
                types
                    .register_tick_planner(id, Arc::clone(planner))
                    .map_err(entity_error)?;
            }
        }
        for (key, policy) in &self.transfer_policies {
            let id = catalog
                .entity_type_id_by_key(key)
                .ok_or_else(|| entity_error(EntityError::InvalidType))?;
            types
                .register_transfer_policy(id, Arc::clone(policy))
                .map_err(entity_error)?;
        }
        types.freeze().map(Arc::new).map_err(entity_error)
    }

    /// Freezes the registered effect kinds before gameplay starts. The
    /// notification kinds entity plans may emit are startup declarations,
    /// alongside the content catalog and entity types.
    pub(super) fn effect_kinds(&self) -> io::Result<EffectKindRegistryFrozen> {
        let mut registry = EffectKindRegistry::new();
        super::entities::register_wake_kind(&mut registry).map_err(|error| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("wake effect kind: {error:?}"),
            )
        })?;
        Ok(registry.freeze())
    }

    pub(super) fn phase_plan(&self) -> io::Result<PhasePlan> {
        let mut registry = SystemRegistry::new();
        builtins::register_builtin_systems(&mut registry)?;
        for (descriptor, handler) in &self.systems {
            if matches!(descriptor.partition(), OwnerPartition::Global)
                || descriptor.writes().is_empty()
                || descriptor.max_effects_per_tick() != 0
                || descriptor.neighbor_radius_chunks() != 0
            {
                return Err(io::Error::new(
                    ErrorKind::Unsupported,
                    format!(
                        "registered owner system {} requires unsupported live capabilities",
                        descriptor.id().as_str()
                    ),
                ));
            }
            registry
                .register_shared_handler(descriptor.clone(), Arc::clone(handler))
                .map_err(|error| {
                    io::Error::new(
                        ErrorKind::InvalidInput,
                        format!("system registry: {error:?}"),
                    )
                })?;
        }
        let plan = registry.freeze().map_err(|error| {
            io::Error::new(
                ErrorKind::InvalidInput,
                format!("system registry: {error:?}"),
            )
        })?;
        let mut seen = BTreeSet::new();
        let mut owner_counts = BTreeMap::<SystemId, usize>::new();
        for (id, owner, _) in &self.owners {
            let system = plan.system(id).ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner seed has no registered system",
                )
            })?;
            if system.driver().is_some()
                || !system.has_executable_handler()
                || !system.accepts_owner(*owner)
                || !seen.insert((id.clone(), *owner))
            {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "invalid or duplicate registered owner seed",
                ));
            }
            let count = owner_counts.entry(id.clone()).or_default();
            *count += 1;
            if *count > MAX_OWNER_VALUES_PER_SYSTEM {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "registered owner seed limit exceeded",
                ));
            }
        }
        Ok(plan)
    }

    pub(super) fn install_owners(self, runtime: &mut SystemRuntime) -> io::Result<()> {
        for (system, owner, value) in self.owners {
            runtime.insert_owner_data(system, owner, value)?;
        }
        Ok(())
    }
}

fn entity_error(error: EntityError) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, error)
}

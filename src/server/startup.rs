//! Registration boundary between a world's content map and live server code.
//!
//! Built-ins and startup extensions freeze together before storage opens.
//! This is an internal trusted-native hook, not public mod loading or a
//! durable owner-state contract.

use super::builtins;
use super::durable::Durability;
use super::effects::{EffectKindRegistry, EffectKindRegistryFrozen};
use super::entities::{
    EntityError, EntityInteractionPolicy, EntityOwnership, EntityPayloadCodec, EntityTickPolicy,
    EntityTransferPolicy, EntityTypeRegistration, EntityTypeRegistry, EntityTypeRegistryBuilder,
    TickPolicy,
};
use super::journal::Transaction;
use super::parallel::{OwnerData, OwnerKey};
use super::registry::{
    OwnerPartition, PhasePlan, SystemDescriptor, SystemHandler, SystemId, SystemRegistry,
};
use super::runtime::owner_codec::OwnerValueCodec;
use super::runtime::owner_durable::OwnerSystemConfig;
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
    storage: Vec<bloxgloom_host_api::StorageBlockEntity>,
    entity_types: Vec<StartupEntityType>,
    transfer_policies: Vec<(String, Arc<dyn EntityTransferPolicy>)>,
    systems: Vec<(SystemDescriptor, Arc<dyn SystemHandler>)>,
    owner_codecs: BTreeMap<SystemId, StartupOwnerCodec>,
    owners: Vec<(SystemId, OwnerKey, OwnerData)>,
}

/// Value codec for one registered owner system. Values live decoded in the
/// durable store; the codec serializes them only for WAL change values, with
/// a declared per-system byte bound enforced fail-closed at insert and patch
/// time. Every registered system needs one before its first seed or wave.
pub(in crate::server) struct StartupOwnerCodec {
    pub(in crate::server) codec: Arc<dyn OwnerValueCodec>,
    pub(in crate::server) codec_version: u16,
    pub(in crate::server) max_bytes: usize,
}

impl ServerStartup {
    /// The development host seam. The package receives only the public registrar.
    #[allow(
        dead_code,
        reason = "Entry point for explicitly installed extension packages; no dynamic loader yet."
    )]
    pub(crate) fn with_extension(
        mut self,
        extension: &dyn bloxgloom_host_api::Extension,
    ) -> io::Result<Self> {
        let mut catalog = (*self.catalog).clone();
        let registration = super::lifecycle::Registration::install(extension, &mut catalog)
            .map_err(io::Error::other)?;
        let mut storage = self.storage.clone();
        storage.extend(registration.definitions);
        super::lifecycle::Registry::resolve(&catalog, &storage).map_err(io::Error::other)?;
        self.catalog = Arc::new(catalog);
        self.storage = storage;
        Ok(self)
    }

    pub(super) fn lifecycles(&self, catalog: &Catalog) -> io::Result<super::lifecycle::Registry> {
        super::lifecycle::Registry::resolve(catalog, &self.storage).map_err(io::Error::other)
    }

    pub(super) fn block_actions_for(
        &self,
        catalog: &Catalog,
    ) -> io::Result<(
        super::block_actions::BlockActionRegistry,
        super::lifecycle::Registry,
    )> {
        use super::block_actions::{BlockActionHooks, BlockActionRegistryBuilder};
        use super::durable::actions::{storage_lifecycle, workstation};
        let mut actions = BlockActionRegistryBuilder::new(catalog);
        for block in [
            crate::content::KILN_BLOCK_TYPE,
            crate::content::HOPPER_BLOCK_TYPE,
        ] {
            actions.register(
                block,
                BlockActionHooks::new(workstation::plan_place, workstation::plan_break),
            )?;
        }
        let lifecycles = self.lifecycles(catalog)?;
        for lifecycle in lifecycles.entries.values() {
            actions.register(
                lifecycle.block,
                BlockActionHooks::new(storage_lifecycle::plan_place, storage_lifecycle::plan_break),
            )?;
        }
        Ok((actions.freeze(), lifecycles))
    }

    pub(crate) fn new(catalog: Arc<Catalog>) -> Self {
        let mut registration = super::lifecycle::Registration::default();
        bloxgloom_host_api::Extension::register(&super::entities::chest::Chest, &mut registration)
            .expect("valid builtin storage declarations");
        Self {
            catalog,
            storage: registration.definitions,
            entity_types: Vec::new(),
            transfer_policies: Vec::new(),
            systems: Vec::new(),
            owner_codecs: BTreeMap::new(),
            owners: Vec::new(),
        }
    }

    #[allow(
        dead_code,
        reason = "Trusted startup extensions register entity implementations here."
    )]
    pub(crate) fn register_entity_type(&mut self, registration: StartupEntityType) {
        self.entity_types.push(registration);
    }

    /// Registers the pure exchange hooks for one entity type by content key.
    /// Kept separate from [`StartupEntityType`] so existing registrations are
    /// untouched: a passive store needs no schedule to be a transfer
    /// endpoint, and ticking types opt in independently of their planner.
    #[allow(
        dead_code,
        reason = "Trusted startup extensions opt entity types into transfers here."
    )]
    pub(crate) fn register_entity_transfer_policy(
        &mut self,
        key: String,
        policy: Arc<dyn EntityTransferPolicy>,
    ) {
        self.transfer_policies.push((key, policy));
    }

    #[allow(
        dead_code,
        reason = "Trusted startup extensions register executable owner systems here."
    )]
    pub(crate) fn register_system<H: SystemHandler>(
        &mut self,
        descriptor: SystemDescriptor,
        handler: H,
    ) {
        self.systems.push((descriptor, Arc::new(handler)));
    }

    #[allow(
        dead_code,
        reason = "Trusted startup extensions seed journaled owner state here."
    )]
    pub(crate) fn seed_owner<T: Any + Send + Sync>(
        &mut self,
        system: SystemId,
        owner: OwnerKey,
        value: T,
    ) {
        self.owners.push((system, owner, OwnerData::new(value)));
    }

    /// Registers the value codec for one owner system. Recovery needs every
    /// live system's codec before replaying the first owner key, so a
    /// registered system without one is a startup error, not a silent
    /// transient fallback.
    #[allow(
        dead_code,
        reason = "Startup owner extensions must install their codecs before recovery."
    )]
    pub(in crate::server) fn register_owner_codec(
        &mut self,
        system: SystemId,
        codec: StartupOwnerCodec,
    ) {
        self.owner_codecs.insert(system, codec);
    }

    /// Builds the durable-store descriptors for every registered system.
    /// Called before storage opens so recovery can rebuild owner state from
    /// the main journal's latest values.
    pub(super) fn owner_configs(&self) -> io::Result<Vec<OwnerSystemConfig>> {
        let mut configs = Vec::with_capacity(self.systems.len());
        for (descriptor, _) in &self.systems {
            let system = descriptor.id();
            let Some(codec) = self.owner_codecs.get(system) else {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "registered owner system {} has no owner codec",
                        system.as_str()
                    ),
                ));
            };
            configs.push(OwnerSystemConfig::new(
                system.clone(),
                Arc::clone(&codec.codec),
                codec.codec_version,
                codec.max_bytes,
                descriptor.partition(),
            )?);
        }
        Ok(configs)
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
        super::entities::mossbun::register(&mut types, &catalog).map_err(entity_error)?;
        super::entities::hopper::register(&mut types, &catalog).map_err(io::Error::other)?;
        for definition in self.lifecycles(&catalog)?.entries.values() {
            super::entities::container::register(&mut types, &catalog, definition)
                .map_err(io::Error::other)?;
        }
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
            if matches!(descriptor.partition(), OwnerPartition::Global) {
                return Err(io::Error::new(
                    ErrorKind::Unsupported,
                    format!(
                        "registered owner system {} uses unsupported global partition",
                        descriptor.id().as_str()
                    ),
                ));
            }
            if descriptor.writes().is_empty() {
                return Err(io::Error::new(
                    ErrorKind::Unsupported,
                    format!(
                        "registered owner system {} declares no owner-state write",
                        descriptor.id().as_str()
                    ),
                ));
            }
            if descriptor.neighbor_radius_chunks() != 0 {
                return Err(io::Error::new(
                    ErrorKind::Unsupported,
                    format!(
                        "registered owner system {} declares unsupported neighbor snapshots (radius {})",
                        descriptor.id().as_str(),
                        descriptor.neighbor_radius_chunks()
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

    pub(super) fn install_owners(
        self,
        runtime: &mut SystemRuntime,
        durability: &mut Durability,
    ) -> io::Result<()> {
        // Seeds journal fresh cells through the main WAL before serving: a
        // restart before the first wave must still recover them. Recovered
        // cells win — the WAL is authoritative — so a seed for an existing
        // cell only checks that the registered value type still matches.
        let mut fresh = Vec::new();
        for (system, owner, value) in self.owners {
            match runtime.owner_snapshot(&system, owner) {
                Some((_, current)) => {
                    if !current.same_type(&value) {
                        return Err(io::Error::new(
                            ErrorKind::InvalidData,
                            format!(
                                "recovered owner state for {} has an unexpected value type",
                                system.as_str()
                            ),
                        ));
                    }
                }
                None => fresh.push((system, owner, value)),
            }
        }
        if fresh.is_empty() {
            return Ok(());
        }
        // Every fallible check runs before the transaction ID is spent, so a
        // rejected seed reserves nothing and stages nothing.
        let mut changes = Vec::with_capacity(fresh.len());
        for (system, owner, value) in &fresh {
            changes.push(runtime.stage_owner_insert(system, *owner, value)?);
        }
        let id = durability.next_id;
        durability.next_id = id
            .checked_add(1)
            .ok_or_else(|| io::Error::other("durable transaction IDs exhausted"))?;
        let receiver = durability
            .writer
            .try_submit(Transaction::new(id, 0, changes))
            .map_err(|error| match error {
                super::journal::SubmitError::Full => io::Error::new(
                    ErrorKind::WouldBlock,
                    "durable journal is full; owner seeds defer",
                ),
                super::journal::SubmitError::Closed => {
                    io::Error::other("durable journal writer is closed")
                }
                super::journal::SubmitError::Invalid(error) => error,
            })?;
        let receipt = receiver
            .recv()
            .map_err(|_| io::Error::other("durable journal worker stopped"))??;
        let _ = receipt.sequence;
        for (system, owner, value) in fresh {
            runtime.insert_owner_data(system, owner, value)?;
        }
        Ok(())
    }
}

fn entity_error(error: EntityError) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, error)
}

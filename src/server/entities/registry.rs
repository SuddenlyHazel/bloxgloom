use super::store::EntitySnapshot;
use super::transfer::{EntityItemTransfer, EntityTransferPolicy};
use super::types::{
    CellCoord, EntityError, EntityId, EntityOwnership, EntityPayload, EntityView,
    MAX_ENTITY_FOOTPRINT_CELLS, MAX_ENTITY_PAYLOAD_BYTES, MAX_ENTITY_PUBLIC_VIEW_BYTES, TickPolicy,
};
use crate::content::{BlockStateId, Catalog, EntityTypeId};
use crate::inventory::Inventory;
use crate::server::registry::MAX_NEIGHBOR_RADIUS;
use crate::server::voxel_view::VoxelView;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const MAX_ENTITY_INTERACTION_REQUEST_BYTES: usize = 256;

/// A validated cell read/write intent emitted by a trusted entity policy.
/// `before` is checked against the resident world before staging; identical
/// before/after values are read preconditions and are not journal writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityBlockStateChange {
    pub cell: CellCoord,
    pub before: BlockStateId,
    pub after: BlockStateId,
}

#[derive(Clone, Debug)]
pub struct EntityInteractionPlan {
    pub payload: EntityPayload,
    pub inventory: Inventory,
    pub block_states: Vec<EntityBlockStateChange>,
    /// Notification-only wake requests for neighbour entities. Each ID is
    /// routed through the registered `bloxgloom:wake_entity` effect kind and
    /// delivered as a bounded transient tick attempt: the destination runs
    /// its own durable work sooner. Lost hints cannot erase due or suspended
    /// recheck eligibility. Wakes carry no state and authorize no writes.
    pub wakes: Vec<EntityId>,
}

#[derive(Clone, Debug)]
pub struct EntityTickPlan {
    pub payload: Option<EntityPayload>,
    /// `Some(tick)` schedules the next due tick; `None` suspends the entity
    /// from ordinary due ticks. Suspended tick policies are re-evaluated by a
    /// bounded circular lane (or earlier by wake hints), so state dependencies
    /// cannot require lossless notification delivery. Unchanged sleeping
    /// entities reaffirm without WAL writes. Rechecks do not catch up missed
    /// physics steps; any resumed work runs through the normal commit path.
    pub next_tick: Option<u64>,
    pub anchor_update: Option<super::types::AnchorUpdate>,
    /// Same-owner mobile position change. A cross-chunk move is staged by
    /// the trusted layer as a fenced barrier transfer instead. Anchored
    /// planners must leave this empty and use `anchor_update`.
    pub position: Option<[f32; 3]>,
    pub block_states: Vec<EntityBlockStateChange>,
    /// Notification-only wake requests, with the same delivery contract as
    /// [`EntityInteractionPlan::wakes`].
    pub wakes: Vec<EntityId>,
    /// Optional push or pull of items with one visible neighbour. The planner only
    /// declares intent; the trusted durable layer resolves both snapshots,
    /// runs both pure exchange hooks, and stages both payload updates as one
    /// atomic batch. `None` plans the tick's own payload alone.
    pub transfer: Option<EntityItemTransfer>,
}

/// Trusted server-only policy for bounded client requests directed at an
/// entity. The callback receives immutable snapshots and cannot perform I/O.
///
/// Implementations must be pure functions of their inputs: no wall clock,
/// RNG, I/O, or global/thread-local state. The views hold only the declared
/// read set; reads outside them fail closed and must surface as
/// `EntityError::ViewOutOfRange`.
pub trait EntityInteractionPolicy: Send + Sync + 'static {
    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        request: &[u8],
        inventory: &Inventory,
        catalog: &Catalog,
        view: &VoxelView,
        neighbours: &EntityView,
    ) -> Result<EntityInteractionPlan, EntityError>;

    /// Chunk read radius around the entity's chunk, captured by the
    /// coordinator before planning. Bounded by `MAX_NEIGHBOR_RADIUS` and
    /// validated at registration time.
    fn read_radius_chunks(&self) -> u8 {
        0
    }

    /// Whether the planner needs the complete public neighbour set.
    fn reads_neighbours(&self) -> bool {
        true
    }
}

/// Trusted deterministic planner for one due tick. The coordinator validates
/// the returned footprint, block preimages, due-time, and payload before WAL.
///
/// Implementations must be pure functions of their inputs: no wall clock,
/// RNG, I/O, or global/thread-local state. The views hold only the declared
/// read set; reads outside them fail closed and must surface as
/// `EntityError::ViewOutOfRange`.
pub trait EntityTickPolicy: Send + Sync + 'static {
    /// Recheck terrain dependencies even while a future due tick is pending.
    /// The planner must avoid advancing ordinary behavior on harmless hints.
    fn wakes_on_terrain_change(&self) -> bool {
        false
    }

    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        current_tick: u64,
        catalog: &Catalog,
        view: &VoxelView,
        neighbours: &EntityView,
    ) -> Result<EntityTickPlan, EntityError>;

    /// Chunk read radius around the entity's chunk, captured by the
    /// coordinator before planning. Bounded by `MAX_NEIGHBOR_RADIUS` and
    /// validated at registration time.
    fn read_radius_chunks(&self) -> u8 {
        0
    }

    /// Whether the planner needs the complete public neighbour set.
    fn reads_neighbours(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityCodecError {
    InvalidData,
    UnsupportedVersion,
}

/// Type-specific payload decoding, migration, persistence encoding, and public projection.
///
/// Payloads stay decoded in the live store; codecs serialize only for WAL and
/// checkpoint values or the bounded public view.
pub trait EntityPayloadCodec: Send + Sync + 'static {
    /// Optional type-specific spatial bounds, checked for preparation and recovery.
    fn validate_location(
        &self,
        _location: &super::types::EntityLocation,
    ) -> Result<(), EntityError> {
        Ok(())
    }

    fn decode(&self, payload: &[u8]) -> Result<EntityPayload, EntityCodecError>;

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError>;

    fn migrate(
        &self,
        from_version: u16,
        to_version: u16,
        payload: &[u8],
    ) -> Result<Vec<u8>, EntityCodecError> {
        if from_version == to_version {
            Ok(payload.to_vec())
        } else {
            Err(EntityCodecError::UnsupportedVersion)
        }
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError>;
}

#[derive(Clone)]
pub struct EntityTypeDescriptor {
    id: EntityTypeId,
    key: String,
    schema_version: u16,
    schema_fingerprint: u64,
    ownership: EntityOwnership,
    tick_policy: TickPolicy,
    max_payload_bytes: usize,
    codec: Arc<dyn EntityPayloadCodec>,
    interaction_policy: Option<Arc<dyn EntityInteractionPolicy>>,
    interaction_read_radius: u8,
    tick_planner: Option<Arc<dyn EntityTickPolicy>>,
    tick_read_radius: u8,
    interaction_reads_neighbours: bool,
    tick_reads_neighbours: bool,
    transfer_policy: Option<Arc<dyn EntityTransferPolicy>>,
}

impl EntityTypeDescriptor {
    pub fn wakes_on_terrain_change(&self) -> bool {
        self.tick_planner
            .as_ref()
            .is_some_and(|planner| planner.wakes_on_terrain_change())
    }

    pub fn validate_location(
        &self,
        location: &super::types::EntityLocation,
    ) -> Result<(), EntityError> {
        self.codec.validate_location(location)
    }

    pub const fn id(&self) -> EntityTypeId {
        self.id
    }

    #[allow(
        dead_code,
        reason = "Frozen type metadata is available to startup extensions."
    )]
    pub fn key(&self) -> &str {
        &self.key
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub const fn schema_fingerprint(&self) -> u64 {
        self.schema_fingerprint
    }

    pub fn ownership(&self) -> &EntityOwnership {
        &self.ownership
    }

    pub const fn tick_policy(&self) -> TickPolicy {
        self.tick_policy
    }

    #[allow(
        dead_code,
        reason = "Extensions can inspect the frozen type's declared payload bound."
    )]
    pub const fn max_payload_bytes(&self) -> usize {
        self.max_payload_bytes
    }

    pub const fn has_interaction_policy(&self) -> bool {
        self.interaction_policy.is_some()
    }

    pub const fn has_tick_planner(&self) -> bool {
        self.tick_planner.is_some()
    }

    /// Pure exchange hooks for one transfer endpoint, resolved by the trusted
    /// durable layer. Policies never see this: they only declare intent.
    /// `None` is not an error by itself: a tick plan naming this type as a
    /// transfer endpoint is rejected as a planner bug instead.
    pub fn transfer_policy(&self) -> Option<&Arc<dyn EntityTransferPolicy>> {
        self.transfer_policy.as_ref()
    }

    /// Declared chunk read radius captured for interaction planning.
    pub const fn interaction_read_radius(&self) -> u8 {
        self.interaction_read_radius
    }

    /// Declared chunk read radius captured for tick planning.
    pub const fn tick_read_radius(&self) -> u8 {
        self.tick_read_radius
    }

    pub const fn interaction_reads_neighbours(&self) -> bool {
        self.interaction_reads_neighbours
    }

    pub const fn tick_reads_neighbours(&self) -> bool {
        self.tick_reads_neighbours
    }

    pub fn plan_interaction(
        &self,
        snapshot: &EntitySnapshot,
        request: &[u8],
        inventory: &Inventory,
        catalog: &Catalog,
        view: &VoxelView,
        neighbours: &EntityView,
    ) -> Result<EntityInteractionPlan, EntityError> {
        if request.is_empty() || request.len() > MAX_ENTITY_INTERACTION_REQUEST_BYTES {
            return Err(EntityError::InvalidPayload);
        }
        self.interaction_policy
            .as_ref()
            .ok_or(EntityError::InvalidType)?
            .plan(snapshot, request, inventory, catalog, view, neighbours)
    }

    pub fn plan_tick(
        &self,
        snapshot: &EntitySnapshot,
        current_tick: u64,
        catalog: &Catalog,
        view: &VoxelView,
        neighbours: &EntityView,
    ) -> Result<EntityTickPlan, EntityError> {
        self.tick_planner
            .as_ref()
            .ok_or(EntityError::InvalidType)?
            .plan(snapshot, current_tick, catalog, view, neighbours)
    }

    pub fn encode_payload(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityError> {
        let encoded = self
            .codec
            .encode(payload)
            .map_err(|_| EntityError::CodecRejected)?;
        if encoded.len() > self.max_payload_bytes {
            return Err(EntityError::PayloadTooLarge);
        }
        Ok(encoded)
    }

    pub fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityError> {
        self.encode_payload(payload)?;
        let view = self
            .codec
            .public_view(payload)
            .map_err(|_| EntityError::CodecRejected)?;
        if view.len() > MAX_ENTITY_PUBLIC_VIEW_BYTES {
            return Err(EntityError::PublicViewTooLarge);
        }
        Ok(view)
    }

    pub fn decode_payload(
        &self,
        stored_version: u16,
        payload: &[u8],
    ) -> Result<EntityPayload, EntityError> {
        if payload.len() > self.max_payload_bytes {
            return Err(EntityError::PayloadTooLarge);
        }
        if stored_version == 0 || stored_version > self.schema_version {
            return Err(EntityError::CodecRejected);
        }
        let migrated = if stored_version == self.schema_version {
            payload.to_vec()
        } else {
            self.codec
                .migrate(stored_version, self.schema_version, payload)
                .map_err(|_| EntityError::CodecRejected)?
        };
        if migrated.len() > self.max_payload_bytes {
            return Err(EntityError::PayloadTooLarge);
        }
        let decoded = self
            .codec
            .decode(&migrated)
            .map_err(|_| EntityError::CodecRejected)?;
        if self.encode_payload(&decoded)? != migrated {
            return Err(EntityError::InvalidPayload);
        }
        Ok(decoded)
    }
}

/// Registration parameters paired with an already registered content entity.
pub struct EntityTypeRegistration {
    pub id: EntityTypeId,
    pub ownership: EntityOwnership,
    pub tick_policy: TickPolicy,
    pub max_payload_bytes: usize,
    pub codec: Arc<dyn EntityPayloadCodec>,
}

/// Startup-only type registry builder. Freezing requires one lifecycle
/// implementation for every entity type in the resolved content catalog.
pub struct EntityTypeRegistryBuilder<'a> {
    catalog: &'a Catalog,
    descriptors: BTreeMap<EntityTypeId, EntityTypeDescriptor>,
}

impl<'a> EntityTypeRegistryBuilder<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self {
            catalog,
            descriptors: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, registration: EntityTypeRegistration) -> Result<(), EntityError> {
        if self.descriptors.contains_key(&registration.id) {
            return Err(EntityError::DuplicateType(registration.id));
        }
        let content_type = self
            .catalog
            .entity_type(registration.id)
            .ok_or(EntityError::UnknownType(registration.id))?;
        if content_type.schema_version == 0
            || content_type.key.is_empty()
            || registration.max_payload_bytes > MAX_ENTITY_PAYLOAD_BYTES
            || matches!(registration.tick_policy, TickPolicy::Interval(0))
        {
            return Err(EntityError::InvalidType);
        }
        match &registration.ownership {
            EntityOwnership::Mobile => {}
            EntityOwnership::Anchored {
                compatible_anchor_states,
                max_footprint_cells,
            } => {
                if compatible_anchor_states.is_empty()
                    || *max_footprint_cells == 0
                    || *max_footprint_cells > MAX_ENTITY_FOOTPRINT_CELLS
                    || compatible_anchor_states
                        .iter()
                        // Deliberate: the anchor state is the block that
                        // represents the entity in the world, so air (0) and
                        // unknown states can never anchor an entity.
                        .any(|state| state.0 == 0 || self.catalog.state(*state).is_none())
                {
                    return Err(EntityError::InvalidType);
                }
            }
        }
        let descriptor = EntityTypeDescriptor {
            id: registration.id,
            key: content_type.key.to_string(),
            schema_version: content_type.schema_version,
            schema_fingerprint: content_type.schema_fingerprint,
            ownership: registration.ownership,
            tick_policy: registration.tick_policy,
            max_payload_bytes: registration.max_payload_bytes,
            codec: registration.codec,
            interaction_policy: None,
            interaction_read_radius: 0,
            tick_planner: None,
            tick_read_radius: 0,
            interaction_reads_neighbours: false,
            tick_reads_neighbours: false,
            transfer_policy: None,
        };
        self.descriptors.insert(registration.id, descriptor);
        Ok(())
    }

    pub fn register_interaction_policy(
        &mut self,
        id: EntityTypeId,
        policy: Arc<dyn EntityInteractionPolicy>,
    ) -> Result<(), EntityError> {
        let descriptor = self
            .descriptors
            .get_mut(&id)
            .ok_or(EntityError::UnknownType(id))?;
        if descriptor.interaction_policy.is_some() {
            return Err(EntityError::DuplicateType(id));
        }
        if policy.read_radius_chunks() > MAX_NEIGHBOR_RADIUS {
            return Err(EntityError::InvalidType);
        }
        descriptor.interaction_read_radius = policy.read_radius_chunks();
        descriptor.interaction_reads_neighbours = policy.reads_neighbours();
        descriptor.interaction_policy = Some(policy);
        Ok(())
    }

    pub fn register_tick_planner(
        &mut self,
        id: EntityTypeId,
        planner: Arc<dyn EntityTickPolicy>,
    ) -> Result<(), EntityError> {
        let descriptor = self
            .descriptors
            .get_mut(&id)
            .ok_or(EntityError::UnknownType(id))?;
        if descriptor.tick_planner.is_some() {
            return Err(EntityError::DuplicateType(id));
        }
        if matches!(descriptor.tick_policy, TickPolicy::Never) {
            return Err(EntityError::InvalidType);
        }
        if planner.read_radius_chunks() > MAX_NEIGHBOR_RADIUS {
            return Err(EntityError::InvalidType);
        }
        descriptor.tick_read_radius = planner.read_radius_chunks();
        descriptor.tick_reads_neighbours = planner.reads_neighbours();
        descriptor.tick_planner = Some(planner);
        Ok(())
    }

    /// Registers the pure exchange hooks that let this type give and receive
    /// items through the atomic transfer plan. Unlike tick planners this is
    /// available to never-ticking types too: a passive store needs no
    /// schedule to be a transfer endpoint.
    pub fn register_transfer_policy(
        &mut self,
        id: EntityTypeId,
        policy: Arc<dyn EntityTransferPolicy>,
    ) -> Result<(), EntityError> {
        let descriptor = self
            .descriptors
            .get_mut(&id)
            .ok_or(EntityError::UnknownType(id))?;
        if descriptor.transfer_policy.is_some() {
            return Err(EntityError::DuplicateType(id));
        }
        descriptor.transfer_policy = Some(policy);
        Ok(())
    }

    pub fn freeze(self) -> Result<EntityTypeRegistry, EntityError> {
        for (kind, id, _, _) in self.catalog.identities() {
            if kind == b'E' {
                let id = EntityTypeId(id);
                if !self.descriptors.contains_key(&id) {
                    return Err(EntityError::MissingTypeRegistration(id));
                }
            }
        }
        if self
            .descriptors
            .keys()
            .any(|id| self.catalog.entity_type(*id).is_none())
        {
            return Err(EntityError::InvalidType);
        }
        Ok(EntityTypeRegistry {
            descriptors: self.descriptors,
        })
    }
}

#[derive(Clone)]
pub struct EntityTypeRegistry {
    descriptors: BTreeMap<EntityTypeId, EntityTypeDescriptor>,
}

impl EntityTypeRegistry {
    pub fn descriptor(&self, id: EntityTypeId) -> Result<&EntityTypeDescriptor, EntityError> {
        self.descriptors
            .get(&id)
            .ok_or(EntityError::UnknownRequiredType(id))
    }

    #[allow(
        dead_code,
        reason = "Startup extensions can inspect the frozen entity vocabulary."
    )]
    pub fn descriptors(&self) -> impl Iterator<Item = &EntityTypeDescriptor> {
        self.descriptors.values()
    }

    pub fn tickable_types(&self) -> impl Iterator<Item = EntityTypeId> + '_ {
        self.descriptors
            .values()
            .filter(|descriptor| descriptor.has_tick_planner())
            .map(EntityTypeDescriptor::id)
    }
}

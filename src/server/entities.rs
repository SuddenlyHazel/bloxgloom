//! Sparse, server-owned entity lifecycle primitives.
//!
//! Entity type declarations, persistence, indexing, and transaction preparation
//! live here so feature modules can share one ownership and revision contract.

pub(in crate::server) mod anchored;
mod checkpoint;
pub(in crate::server) mod chest;
mod codec;
mod gameplay;
pub(in crate::server) use gameplay::Codec as GameplayCodec;
pub(in crate::server) mod container;
#[cfg(test)]
pub(in crate::server) mod hopper;
#[cfg(test)]
mod kiln;
mod locomotion;
pub(in crate::server) mod machine;
pub(in crate::server) mod mobile;
mod mobile_pages;
#[cfg(test)]
pub(in crate::server) mod mossbun;
mod navigation;
mod persistence;
mod player;
mod storage;
pub(in crate::server) use mobile_pages::MobilePage;
mod registry;
mod spatial;
mod store;
mod transfer;
mod types;
pub(super) mod wake;

#[cfg(test)]
mod tests;

pub(super) use checkpoint::EntityCheckpointStore;
#[cfg(test)]
pub(super) use kiln::{
    KilnFacing, KilnPayload, kiln_block_states, kiln_footprint, kiln_payload,
    register_entity_type as register_kiln_entity_type,
};
#[cfg(test)]
pub(super) use kiln::{KilnSlot, plan_insert};
pub(super) use persistence::write_checkpoint;
pub(super) use persistence::{decode_checkpoint, decode_motion_value, encode_checkpoint};
#[cfg(test)]
pub(super) use player::PlayerEntityPayload;
pub(super) use player::{PlayerEntityStore, register_player_entity_type};
pub(super) use registry::{
    EntityBlockStateChange, EntityCodecError, EntityInteractionPlan, EntityInteractionPolicy,
    EntityPayloadCodec, EntityTickPlan, EntityTickPolicy, EntityTypeDescriptor,
    EntityTypeRegistration, EntityTypeRegistry, EntityTypeRegistryBuilder,
    MAX_ENTITY_INTERACTION_REQUEST_BYTES,
};
pub(super) use store::{
    ENTITY_ALLOCATOR_DOMAIN, ENTITY_CELL_DOMAIN, ENTITY_CHUNK_DOMAIN, ENTITY_MOTION_DOMAIN,
    ENTITY_RECORD_DOMAIN, ENTITY_REVISION_DOMAIN, EntityCommit, EntityDelta, EntityDependencies,
    EntityPatch, EntitySnapshot, EntitySpawn, EntityStore, PreparedEntityBatch,
    PreparedEntityTransaction, cell_state_key,
};
pub(super) use transfer::{EntityItemTransfer, EntityTransferPolicy};
pub(super) use types::{
    AnchorUpdate, CellCoord, EntityError, EntityId, EntityLocation, EntityOwnership, EntityPayload,
    EntityPublicView, EntityView, MAX_PLAN_NEIGHBOUR_BYTES, MAX_PLAN_NEIGHBOURS, TickPolicy,
    position_to_cell,
};
#[cfg(test)]
pub(super) use types::{EntityMotionSnapshot, EntityOwner, MAX_ENTITY_PUBLIC_VIEW_BYTES};
pub(super) use wake::{
    canonical_wakes, interact_producer, register_wake_kind, route_wakes, tick_producer,
};

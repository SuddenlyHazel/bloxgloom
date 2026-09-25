//! Sparse, server-owned entity lifecycle primitives.
//!
//! Entity type declarations, persistence, indexing, and transaction preparation
//! live here so feature modules can share one ownership and revision contract.

mod checkpoint;
mod codec;
mod kiln;
mod persistence;
mod player;
mod registry;
mod spatial;
mod store;
mod transfer;
mod types;
pub(super) mod wake;

#[cfg(test)]
mod tests;

pub(super) use checkpoint::EntityCheckpointStore;
pub(super) use kiln::{
    KilnBreakPlan, KilnFacing, KilnHalf, KilnInsertPlan, KilnPayload, KilnRecipe, KilnRecipeBook,
    KilnSlot, KilnTakePlan, KilnTickPlan, kiln_block_states, kiln_entity_type_id, kiln_footprint,
    kiln_payload, kiln_state, plan_break, plan_insert, plan_take, plan_tick,
    register_entity_type as register_kiln_entity_type,
    register_entity_type_with_recipes as register_kiln_entity_type_with_recipes,
};
pub(super) use persistence::{decode_checkpoint, decode_motion_value, encode_checkpoint};
pub(super) use player::{PlayerEntityPayload, PlayerEntityStore, register_player_entity_type};
pub(super) use registry::{
    EntityBlockStateChange, EntityCodecError, EntityInteractionPlan, EntityInteractionPolicy,
    EntityPayloadCodec, EntityTickPlan, EntityTickPolicy, EntityTypeDescriptor,
    EntityTypeRegistration, EntityTypeRegistry, EntityTypeRegistryBuilder,
    MAX_ENTITY_INTERACTION_REQUEST_BYTES,
};
pub(super) use store::{
    ENTITY_ALLOCATOR_DOMAIN, ENTITY_CELL_DOMAIN, ENTITY_CHUNK_DOMAIN, ENTITY_MOTION_DOMAIN,
    ENTITY_RECORD_DOMAIN, ENTITY_REVISION_DOMAIN, EntityCommit, EntityDelta, EntityPatch,
    EntitySnapshot, EntitySpawn, EntityStore, MAX_ENTITY_SPAWN_BATCH, MAX_ENTITY_TRANSACTION_BYTES,
    MAX_ENTITY_TRANSACTION_CHANGES, PreparedEntityBatch, PreparedEntityTransaction,
};
pub(super) use transfer::{EntityItemTransfer, EntityTransferPolicy};
pub(super) use types::{
    AnchorUpdate, CellCoord, EntityError, EntityId, EntityLocation, EntityMotionSnapshot,
    EntityOwner, EntityOwnership, EntityPayload, EntityPublicView, EntityView,
    MAX_ENTITY_FOOTPRINT_CELLS, MAX_ENTITY_PAYLOAD_BYTES, MAX_ENTITY_PUBLIC_VIEW_BYTES,
    MAX_PLAN_NEIGHBOUR_BYTES, MAX_PLAN_NEIGHBOURS, TickPolicy, position_to_cell,
};
pub(super) use wake::{
    canonical_wakes, interact_producer, register_wake_kind, route_wakes, tick_producer,
};

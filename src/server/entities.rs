//! Sparse, server-owned entity lifecycle primitives.
//!
//! Entity type declarations, persistence, indexing, and transaction preparation
//! live here so feature modules can share one ownership and revision contract.

mod codec;
mod persistence;
mod registry;
mod spatial;
mod store;
mod types;

#[cfg(test)]
mod tests;

pub(super) use persistence::{decode_checkpoint, encode_checkpoint};
pub(super) use registry::{
    EntityCodecError, EntityPayloadCodec, EntityTypeDescriptor, EntityTypeRegistration,
    EntityTypeRegistry, EntityTypeRegistryBuilder,
};
pub(super) use store::{
    EntityCommit, EntityDelta, EntityPatch, EntitySnapshot, EntitySpawn, EntityStore,
    PreparedEntityTransaction,
};
pub(super) use types::{
    AnchorUpdate, CellCoord, EntityError, EntityId, EntityLocation, EntityOwner, EntityOwnership,
    EntityPayload, EntityPublicView, MAX_ENTITY_FOOTPRINT_CELLS, MAX_ENTITY_PAYLOAD_BYTES,
    MAX_ENTITY_PUBLIC_VIEW_BYTES, TickPolicy,
};

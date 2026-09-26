//! Anchored two-cell kiln lifecycle and deterministic gameplay planning.
//!
//! Typed state, codec, and planning are split into focused modules. The lower
//! block owns one entity and the complete footprint; live callers must combine
//! plans with world edits/drops before submitting one durable transaction.

mod codec;
mod model;
mod planning;

use super::registry::{EntityTypeRegistration, EntityTypeRegistryBuilder};
use super::types::{CellCoord, EntityError};
use crate::content::{
    BlockStateId, Catalog, KILN_BLOCK_TYPE, KILN_DEFAULT_STATE, KILN_ENTITY_TYPE,
};
use crate::server::entities::EntitySnapshot;
use codec::KilnPayloadCodec;
#[cfg(test)]
use model::{FUEL_SLOT_INDEX, INPUT_SLOT_INDEX, KILN_MAX_COOK_TICKS, OUTPUT_SLOT_INDEX};
use model::{KILN_MAX_PAYLOAD_BYTES, KILN_TICK_INTERVAL};
use model::{KilnFacing as Facing, KilnHalf as Half};
pub(in crate::server) use model::{KilnFacing, KilnHalf, KilnPayload, KilnRecipeBook};
#[cfg(test)]
pub(in crate::server) use model::{KilnRecipe, KilnSlot};
pub(in crate::server) use planning::plan_break;
use planning::{KilnInteractionPolicy, KilnTickPlanner};
#[cfg(test)]
pub(in crate::server) use planning::{plan_insert, plan_take, plan_tick};
use std::sync::Arc;

/// Resolve a canonical state from the compiled `facing/half/lit` lattice.
/// The default kiln state is lower/north/unlit.
pub(in crate::server) fn kiln_state(
    catalog: &Catalog,
    half: KilnHalf,
    facing: KilnFacing,
    lit: bool,
) -> Result<BlockStateId, EntityError> {
    let mut state = KILN_DEFAULT_STATE;
    for (name, value) in [
        ("facing", facing.name()),
        ("half", half.name()),
        ("lit", if lit { "true" } else { "false" }),
    ] {
        state = catalog
            .state_with_property(state, name, value)
            .ok_or(EntityError::InvalidType)?;
    }
    let definition = catalog.state(state).ok_or(EntityError::InvalidType)?;
    if definition.block_type != KILN_BLOCK_TYPE {
        return Err(EntityError::InvalidType);
    }
    Ok(state)
}

pub(in crate::server) fn kiln_block_states(
    catalog: &Catalog,
    payload: &KilnPayload,
) -> Result<[BlockStateId; 2], EntityError> {
    Ok([
        kiln_state(catalog, Half::Lower, payload.facing, payload.lit)?,
        kiln_state(catalog, Half::Upper, payload.facing, payload.lit)?,
    ])
}

pub(in crate::server) fn kiln_footprint(anchor: CellCoord) -> Result<Vec<CellCoord>, EntityError> {
    let upper = CellCoord::new(
        anchor.x,
        anchor
            .y
            .checked_add(1)
            .ok_or(EntityError::InvalidLocation)?,
        anchor.z,
    );
    let mut footprint = vec![anchor, upper];
    footprint.sort_unstable();
    Ok(footprint)
}

pub(in crate::server) fn kiln_payload(snapshot: &EntitySnapshot) -> Option<&KilnPayload> {
    snapshot.private_payload.downcast_ref::<KilnPayload>()
}

/// Register the catalogued anchored kiln type using the shared frozen catalog.
pub(in crate::server) fn register_entity_type(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: Arc<Catalog>,
) -> Result<(), EntityError> {
    let recipes = Arc::new(KilnRecipeBook::builtins(&catalog)?);
    register_entity_type_with_recipes(builder, catalog, recipes)
}

pub(in crate::server) fn register_entity_type_with_recipes(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: Arc<Catalog>,
    recipes: Arc<KilnRecipeBook>,
) -> Result<(), EntityError> {
    let mut compatible_anchor_states = Vec::with_capacity(8);
    for facing in Facing::ALL {
        for lit in [false, true] {
            compatible_anchor_states.push(kiln_state(&catalog, Half::Lower, facing, lit)?);
        }
    }
    builder.register(EntityTypeRegistration {
        id: KILN_ENTITY_TYPE,
        ownership: super::types::EntityOwnership::anchored(compatible_anchor_states, 2),
        tick_policy: super::types::TickPolicy::Interval(KILN_TICK_INTERVAL as u32),
        max_payload_bytes: KILN_MAX_PAYLOAD_BYTES,
        codec: Arc::new(KilnPayloadCodec {
            catalog: Arc::clone(&catalog),
        }),
    })?;
    builder.register_interaction_policy(KILN_ENTITY_TYPE, Arc::new(KilnInteractionPolicy))?;
    builder.register_tick_planner(KILN_ENTITY_TYPE, Arc::new(KilnTickPlanner { recipes }))
}

#[cfg(test)]
#[path = "kiln/tests.rs"]
mod tests;

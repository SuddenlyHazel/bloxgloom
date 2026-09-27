//! Builtin kiln control surface for the generic entity-interaction wire.
//!
//! One `EntityAdapter` among others: it matches aimed kiln blocks and emits
//! the same opaque, bounded payloads the server validates. The server resolves
//! the touched footprint cell and owns every slot check.
//!
//! `KilnCommand`, `is_kiln_hit`, and `interaction` are the historic per-type
//! spellings; they delegate to the registered adapter path so the emitted
//! request bytes stay byte-identical while production input flows through the
//! generic registry.

use super::registry::{EntityAdapter, EntityVerb};
use crate::content::{Catalog, KILN_BLOCK_TYPE, KILN_ENTITY_TYPE};
use crate::protocol::ClientMessage;
use crate::raycast::Hit;

#[derive(Clone, Copy)]
#[cfg(test)]
pub(in crate::client) enum KilnCommand {
    InsertInput,
    TakeOutput,
    InsertFuel,
    TakeFuel,
}

/// Verbs served by the kiln adapter. Key bindings target these through the
/// generic registry instead of importing `KilnCommand`.
pub(in crate::client) const INSERT_INPUT: EntityVerb = "kiln:insert-input";
pub(in crate::client) const TAKE_OUTPUT: EntityVerb = "kiln:take-output";
pub(in crate::client) const INSERT_FUEL: EntityVerb = "kiln:insert-fuel";
pub(in crate::client) const TAKE_FUEL: EntityVerb = "kiln:take-fuel";

pub(in crate::client) fn kiln_adapter() -> EntityAdapter {
    EntityAdapter {
        entity_type: KILN_ENTITY_TYPE,
        project_avatar: no_avatar,
        hit_test: is_kiln_hit,
        interact: interact_verb,
    }
}

pub(in crate::client) fn is_kiln_hit(hit: Hit, catalog: &Catalog) -> bool {
    catalog
        .state(hit.block_id)
        .is_some_and(|state| state.block_type == KILN_BLOCK_TYPE)
}

#[cfg(test)]
pub(in crate::client) fn interaction(
    hit: Hit,
    action_id: u128,
    hotbar_slot: u8,
    command: KilnCommand,
) -> ClientMessage {
    let (operation, kiln_slot) = match command {
        KilnCommand::InsertInput => (0, 1),
        KilnCommand::TakeOutput => (1, 2),
        KilnCommand::InsertFuel => (0, 0),
        KilnCommand::TakeFuel => (1, 0),
    };
    ClientMessage::EntityInteract {
        action_id,
        target: hit.block,
        payload: request_bytes(operation, kiln_slot, hotbar_slot),
    }
}

fn interact_verb(
    hit: Hit,
    action_id: u128,
    hotbar_slot: u8,
    verb: EntityVerb,
) -> Option<ClientMessage> {
    let (operation, kiln_slot) = match verb {
        INSERT_INPUT => (0, 1),
        TAKE_OUTPUT => (1, 2),
        INSERT_FUEL => (0, 0),
        TAKE_FUEL => (1, 0),
        _ => return None,
    };
    Some(ClientMessage::EntityInteract {
        action_id,
        target: hit.block,
        payload: request_bytes(operation, kiln_slot, hotbar_slot),
    })
}

/// The kiln's client replication contract on the wire: `[version, operation,
/// kiln_slot, hotbar_slot, count, reserved]`. Shared by the legacy spelling
/// and the registry path so both emit identical bytes.
fn request_bytes(operation: u8, kiln_slot: u8, hotbar_slot: u8) -> Vec<u8> {
    vec![1, operation, kiln_slot, hotbar_slot, 1, 0]
}

fn no_avatar(
    entity: &crate::protocol::PublicEntity,
) -> Result<Option<crate::render::VisualAvatar>, ()> {
    crate::protocol::workstation::WorkstationView::decode(&entity.payload).ok_or(())?;
    Ok(None)
}

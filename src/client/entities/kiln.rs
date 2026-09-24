//! Minimal builtin control surface for the generic entity-interaction wire.
//! The server resolves the touched footprint cell and owns every slot check.

use crate::content::{Catalog, KILN_BLOCK_TYPE};
use crate::protocol::ClientMessage;
use crate::raycast::Hit;

#[derive(Clone, Copy)]
pub(in crate::client) enum KilnCommand {
    InsertInput,
    TakeOutput,
    InsertFuel,
    TakeFuel,
}

pub(in crate::client) fn is_kiln_hit(hit: Hit, catalog: &Catalog) -> bool {
    catalog
        .state(hit.block_id)
        .is_some_and(|state| state.block_type == KILN_BLOCK_TYPE)
}

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
        payload: vec![1, operation, kiln_slot, hotbar_slot, 1, 0],
    }
}

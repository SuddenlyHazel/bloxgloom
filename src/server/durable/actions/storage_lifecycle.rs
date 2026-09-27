//! Translate public lifecycle effects into the existing atomic host transaction.
use super::BlockEditCommand;
use super::workstation::{self, Placement, Removal};
use crate::inventory::Stack;
use crate::server::block_actions::{BlockActionContext, BlockCommitBuilder};
use crate::server::durable::CommitAction;
use crate::server::entities::container::ContainerPayload;
use crate::server::entities::{CellCoord, EntityLocation, EntityPayload, EntitySpawn};
use crate::server::simulation::TickId;
use crate::world::BlockId;
use bloxgloom_host_api::lifecycle::{PlacementContext, RemovalContext};
use std::io;

pub(in crate::server) fn plan_place(
    context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    tick: TickId,
    command: BlockEditCommand,
    previous: BlockId,
) -> io::Result<CommitAction> {
    let definition = context.lifecycle(command.block)?;
    let catalog = context.catalog();
    let plan = definition
        .definition
        .plan_place(PlacementContext {
            anchor: [command.x, command.y, command.z],
            state: &catalog
                .state(command.block)
                .ok_or_else(|| io::Error::other("unknown placement state"))?
                .key,
        })
        .map_err(io::Error::other)?;
    let anchor = CellCoord::new(command.x, command.y, command.z);
    if plan.consume_item != definition.definition.placement_item {
        return Err(io::Error::other("unregistered placement cost"));
    }
    let cells: Vec<_> = plan
        .cells
        .into_iter()
        .zip(&definition.states)
        .map(|((at, _), state)| (CellCoord::new(at[0], at[1], at[2]), *state))
        .collect();
    let spawn = EntitySpawn::Anchored {
        entity_type: definition.entity,
        anchor,
        anchor_state: definition.anchor,
        footprint: cells.iter().map(|(c, _)| *c).collect(),
        payload: EntityPayload::new(ContainerPayload {
            slots: vec![None; plan.empty_slots],
        }),
        spawn_tick: tick.get(),
    };
    workstation::place(
        context,
        builder,
        tick,
        command,
        previous,
        Placement {
            item: definition.item,
            cells,
            spawn,
        },
    )
}

pub(in crate::server) fn plan_break(
    context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    tick: TickId,
    command: BlockEditCommand,
    previous: BlockId,
) -> io::Result<CommitAction> {
    let definition = context.lifecycle(previous)?;
    let broken = CellCoord::new(command.x, command.y, command.z);
    let snapshot = context
        .entities()
        .anchored_at(broken)
        .and_then(|id| context.entities().snapshot(id))
        .ok_or_else(|| io::Error::other("registered block has no entity"))?;
    if snapshot.entity_type != definition.entity {
        return Err(io::Error::other("lifecycle entity mismatch"));
    }
    let EntityLocation::Anchored {
        anchor,
        ref footprint,
        anchor_state,
    } = snapshot.location
    else {
        return Err(io::Error::other("not anchored"));
    };
    if anchor_state != definition.anchor {
        return Err(io::Error::other("anchor state mismatch"));
    }
    let coordinates: Vec<_> = footprint.iter().map(|c| [c.x, c.y, c.z]).collect();
    let plan = definition
        .definition
        .plan_remove(RemovalContext {
            anchor: [anchor.x, anchor.y, anchor.z],
            touched: [broken.x, broken.y, broken.z],
            footprint: &coordinates,
        })
        .map_err(io::Error::other)?;
    if plan.refund_item != definition.definition.placement_item {
        return Err(io::Error::other("unregistered removal refund"));
    }
    let cells = plan
        .cells
        .into_iter()
        .zip(&definition.states)
        .map(|((at, _), state)| (CellCoord::new(at[0], at[1], at[2]), *state))
        .collect();
    let payload = snapshot
        .private_payload
        .downcast_ref::<ContainerPayload>()
        .filter(|p| p.slots.len() == definition.definition.slots)
        .ok_or_else(|| io::Error::other("invalid storage payload"))?;
    let mut drops = vec![Stack::new(definition.item, 1)];
    drops.extend(payload.slots.iter().flatten().cloned());
    workstation::remove(
        context,
        builder,
        tick,
        command,
        Removal {
            snapshot,
            cells,
            drops,
        },
    )
}

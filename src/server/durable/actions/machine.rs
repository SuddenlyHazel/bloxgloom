//! Registered machine lifecycle, using the common finite-inventory transaction.
use super::{BlockEditCommand, workstation};
use crate::{
    inventory::Stack,
    server::{
        block_actions::{BlockActionContext, BlockCommitBuilder},
        durable::CommitAction,
        entities::{
            CellCoord, EntityLocation, EntityPayload, EntitySpawn, machine::MachinePayload,
        },
        simulation::TickId,
    },
    world::BlockId,
};
use std::io;
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid machine lifecycle")
}
pub(in crate::server) fn place(
    context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    tick: TickId,
    command: BlockEditCommand,
    previous: BlockId,
) -> io::Result<CommitAction> {
    let catalog = context.catalog();
    let block = catalog.state(command.block).ok_or_else(invalid)?.block_type;
    let (id, m) = catalog
        .machines()
        .find(|(_, m)| catalog.block_by_key(&m.block) == Some(block))
        .ok_or_else(invalid)?;
    let key = &catalog.state(command.block).ok_or_else(invalid)?.key;
    let plan = m
        .plan_place([command.x, command.y, command.z], key)
        .map_err(io::Error::other)?;
    let payload = MachinePayload::empty(m.slots, plan.variant);
    let anchor = CellCoord::new(command.x, command.y, command.z);
    let cells = plan
        .cells
        .into_iter()
        .map(|(at, key)| {
            Ok((
                CellCoord::new(at[0], at[1], at[2]),
                catalog.state_by_key(&key).ok_or_else(invalid)?,
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let item = catalog.item_by_key(&plan.item).ok_or_else(invalid)?;
    let spawn = EntitySpawn::Anchored {
        entity_type: id,
        anchor,
        anchor_state: command.block,
        footprint: cells.iter().map(|(c, _)| *c).collect(),
        payload: EntityPayload::new(payload),
        spawn_tick: tick.get(),
    };
    workstation::place(
        context,
        builder,
        tick,
        command,
        previous,
        workstation::Placement {
            item,
            cost: 1,
            cells,
            spawn,
        },
    )
}
pub(in crate::server) fn remove(
    context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    tick: TickId,
    command: BlockEditCommand,
    _: BlockId,
) -> io::Result<CommitAction> {
    let id = context
        .entities()
        .anchored_at(CellCoord::new(command.x, command.y, command.z))
        .ok_or_else(invalid)?;
    let snapshot = context.entities().snapshot(id).ok_or_else(invalid)?;
    let catalog = context.catalog();
    let m = catalog.machine(snapshot.entity_type).ok_or_else(invalid)?;
    let p = snapshot
        .private_payload
        .downcast_ref::<MachinePayload>()
        .ok_or_else(invalid)?;
    let anchor = snapshot.anchor().ok_or_else(invalid)?;
    let EntityLocation::Anchored { ref footprint, .. } = snapshot.location else {
        return Err(invalid());
    };
    let stored: Vec<_> = footprint.iter().map(|c| [c.x, c.y, c.z]).collect();
    let plan = m
        .plan_remove(
            [anchor.x, anchor.y, anchor.z],
            [command.x, command.y, command.z],
            p.variant,
            p.fuel > 0,
            &stored,
        )
        .map_err(io::Error::other)?;
    let cells = plan
        .cells
        .into_iter()
        .map(|(at, key)| {
            Ok((
                CellCoord::new(at[0], at[1], at[2]),
                catalog.state_by_key(&key).ok_or_else(invalid)?,
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let item = catalog.item_by_key(&plan.item).ok_or_else(invalid)?;
    let mut drops = vec![Stack::new(item, 1)];
    drops.extend(p.slots.iter().flatten().cloned());
    workstation::remove(
        context,
        builder,
        tick,
        command,
        workstation::Removal {
            snapshot,
            cells,
            drops,
        },
    )
}

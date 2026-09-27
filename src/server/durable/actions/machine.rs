//! Registered machine lifecycle, using the common finite-inventory transaction.
use super::{BlockEditCommand, workstation};
use crate::{
    inventory::Stack,
    server::{
        block_actions::{BlockActionContext, BlockCommitBuilder},
        durable::CommitAction,
        entities::{
            CellCoord, EntityPayload, EntitySpawn,
            machine::{Adapter, MachinePayload},
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
    let variant = m
        .variants
        .iter()
        .position(|v| catalog.state_by_key(&v.placement_state) == Some(command.block))
        .ok_or_else(invalid)?;
    let payload = MachinePayload::empty(m.slots, variant as u8);
    let anchor = CellCoord::new(command.x, command.y, command.z);
    let adapter = Adapter::new(catalog.clone(), m.clone());
    let cells = adapter.cells(anchor, &payload).map_err(io::Error::other)?;
    let item = catalog
        .items()
        .find(|i| i.key == m.item)
        .ok_or_else(invalid)?
        .id;
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
        workstation::Placement { item, cells, spawn },
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
    let cells = Adapter::new(catalog.clone(), m.clone())
        .cells(anchor, p)
        .map_err(io::Error::other)?;
    let item = catalog
        .items()
        .find(|i| i.key == m.item)
        .ok_or_else(invalid)?
        .id;
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

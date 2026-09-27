//! Placement, removal and scheduled support removal share host-owned economy.
use super::{BlockEditCommand, workstation};
use crate::{
    inventory::Stack,
    server::{
        block_actions::{BlockActionContext, BlockCommitBuilder},
        durable::CommitAction,
        entities::*,
        simulation::TickId,
    },
    world::BlockId,
};
use std::{io, time::Duration};

pub(in crate::server) fn place(
    context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    tick: TickId,
    command: BlockEditCommand,
    previous: BlockId,
) -> io::Result<CommitAction> {
    let catalog = context.catalog();
    let (id, definition) = catalog
        .anchored_for_state(command.block)
        .ok_or_else(|| io::Error::other("missing anchored lifecycle"))?;
    if catalog.state_by_key(&definition.anchor_state) != Some(command.block) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "not a placement state",
        ));
    }
    let anchor = CellCoord::new(command.x, command.y, command.z);
    let adapter = anchored::Adapter {
        definition: definition.clone(),
        catalog: catalog.clone(),
    };
    let cells = adapter.cells(anchor).map_err(io::Error::other)?;
    let payload = definition
        .behavior
        .initialize([anchor.x, anchor.y, anchor.z])
        .map_err(|_| io::Error::other("anchored initialization failed"))?;
    let spawn = EntitySpawn::Anchored {
        entity_type: id,
        anchor,
        anchor_state: command.block,
        footprint: cells.iter().map(|(c, _)| *c).collect(),
        payload,
        spawn_tick: tick.get(),
    };
    let item = catalog
        .items()
        .find(|i| i.key == definition.placement_item)
        .unwrap()
        .id;
    workstation::place(
        context,
        builder,
        tick,
        command,
        previous,
        workstation::Placement {
            item,
            cost: definition.placement_cost,
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
    let catalog = context.catalog();
    let snapshot = context
        .entities()
        .anchored_at(CellCoord::new(command.x, command.y, command.z))
        .and_then(|id| context.entities().snapshot(id))
        .ok_or_else(|| io::Error::other("missing anchored entity"))?;
    let definition = catalog
        .anchored_entity(snapshot.entity_type)
        .ok_or_else(|| io::Error::other("wrong anchored entity type"))?;
    let adapter = anchored::Adapter {
        definition: definition.clone(),
        catalog: catalog.clone(),
    };
    let cells = adapter
        .cells(snapshot.anchor().unwrap())
        .map_err(io::Error::other)?;
    let drops = refund_stacks(
        &catalog,
        definition,
        &snapshot.private_payload,
        bloxgloom_host_api::anchored::RemovalCause::Broken,
    )?;
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
pub(super) fn refund_stacks(
    catalog: &crate::content::Catalog,
    d: &bloxgloom_host_api::anchored::AnchoredBlockEntity,
    payload: &EntityPayload,
    cause: bloxgloom_host_api::anchored::RemovalCause,
) -> io::Result<Vec<Stack>> {
    let count = d
        .behavior
        .refund(payload, cause, d.removal_refund)
        .map_err(|_| io::Error::other("invalid removal callback"))?;
    if count > d.removal_refund {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refund exceeds registered budget",
        ));
    }
    Ok(if count == 0 {
        vec![]
    } else {
        vec![Stack::new(
            catalog
                .items()
                .find(|i| i.key == d.placement_item)
                .unwrap()
                .id,
            count,
        )]
    })
}
pub(super) fn refund_removal(
    state: &crate::server::State,
    snapshot: &EntitySnapshot,
    tick: u64,
    base: PreparedEntityTransaction,
) -> io::Result<PreparedEntityTransaction> {
    let catalog = state.world.catalog();
    let d = catalog
        .anchored_entity(snapshot.entity_type)
        .ok_or_else(|| io::Error::other("missing removal definition"))?;
    let a = snapshot
        .anchor()
        .ok_or_else(|| io::Error::other("missing removal anchor"))?;
    let spawns: Vec<_> = refund_stacks(
        catalog,
        d,
        &snapshot.private_payload,
        bloxgloom_host_api::anchored::RemovalCause::Reaction,
    )?
    .into_iter()
    .map(|stack| {
        (
            [a.x as f32 + 0.5, a.y as f32 + 0.5, a.z as f32 + 0.5],
            stack,
            Duration::from_millis(250),
        )
    })
    .collect();
    match crate::server::drops::plan_stack_spawns(
        &state.entities,
        catalog,
        &spawns,
        tick,
        crate::server::drops::unix_ms(),
    )? {
        Some(drops) => state
            .entities
            .combine_prepared(vec![base, drops])
            .map_err(io::Error::other),
        None => Ok(base),
    }
}

//! Expand system destruction into complete registered-footprint removal. This is
//! transaction assembly, not a second simulation or fire behavior implementation.
use super::*;
use crate::inventory::Stack;
use crate::server::entities::{self, CellCoord as EntityCell, EntityLocation, EntitySnapshot};
use std::collections::BTreeMap;
type RemovalParts = (Vec<(EntityCell, BlockId)>, Vec<Stack>);

fn capacity_error(error: entities::EntityError) -> io::Error {
    let kind = match error {
        entities::EntityError::TransactionTooLarge
        | entities::EntityError::TooManyTransactionChanges
        | entities::EntityError::SpatialQueryTooBroad
        | entities::EntityError::TooManyEntities
        | entities::EntityError::ChunkReferenceBudgetExceeded(_)
        | entities::EntityError::ChunkPublicViewBudgetExceeded(_)
        | entities::EntityError::ChunkPayloadBudgetExceeded(_) => ErrorKind::QuotaExceeded,
        _ => ErrorKind::InvalidData,
    };
    io::Error::new(kind, error)
}

pub(in crate::server) fn removal(
    state: &State,
    snapshot: &EntitySnapshot,
) -> io::Result<RemovalParts> {
    let catalog = state.world.catalog_arc();
    let anchor = snapshot
        .anchor()
        .ok_or_else(|| io::Error::other("not anchored"))?;
    if let Some(d) = catalog.anchored_entity(snapshot.entity_type) {
        let adapter = entities::anchored::Adapter {
            definition: d.clone(),
            catalog: catalog.clone(),
        };
        return Ok((
            adapter.cells(anchor).map_err(io::Error::other)?,
            super::anchored::refund_stacks(
                &catalog,
                d,
                &snapshot.private_payload,
                bloxgloom_host_api::anchored::RemovalCause::WorldEdit,
            )?,
        ));
    }
    if let Some(d) = catalog.machine(snapshot.entity_type) {
        let adapter = entities::machine::Adapter::new(catalog.clone(), d.clone());
        let p = snapshot
            .private_payload
            .downcast_ref::<entities::machine::MachinePayload>()
            .ok_or_else(|| io::Error::other("invalid machine payload"))?;
        let item = catalog.items().find(|i| i.key == d.item).unwrap().id;
        let mut drops = vec![Stack::new(item, 1)];
        drops.extend(p.slots.iter().flatten().cloned());
        return Ok((adapter.cells(anchor, p).map_err(io::Error::other)?, drops));
    }
    let EntityLocation::Anchored { anchor_state, .. } = snapshot.location else {
        unreachable!()
    };
    let d = state
        .lifecycles
        .for_state(&catalog, anchor_state)
        .ok_or_else(|| io::Error::other("unregistered anchored invalidation"))?;
    if d.entity != snapshot.entity_type {
        return Err(io::Error::other("wrong lifecycle entity"));
    }
    let p = snapshot
        .private_payload
        .downcast_ref::<entities::container::ContainerPayload>()
        .ok_or_else(|| io::Error::other("invalid storage payload"))?;
    let plan = d
        .definition
        .plan_place(bloxgloom_host_api::lifecycle::PlacementContext {
            anchor: [anchor.x, anchor.y, anchor.z],
            state: &d.definition.anchor_state,
        })
        .map_err(io::Error::other)?;
    let cells = plan
        .cells
        .into_iter()
        .zip(&d.states)
        .map(|((c, _), s)| (EntityCell::new(c[0], c[1], c[2]), *s))
        .collect();
    let mut drops = vec![Stack::new(d.item, 1)];
    drops.extend(p.slots.iter().flatten().cloned());
    Ok((cells, drops))
}

/// Caller supplies a bounded edit batch. At most 32 unique destructions, 2048
/// footprint cells and 1760 refund/content stacks are captured, before allocation.
pub(in crate::server) fn plan(
    state: &mut State,
    edits: &[(i32, i32, i32, BlockId)],
    tick: u64,
) -> io::Result<Option<CommitAction>> {
    if edits.len() > 32 {
        return Err(io::Error::new(
            ErrorKind::QuotaExceeded,
            "invalidation edit budget",
        ));
    }
    if edits.iter().any(|&(_, _, _, block)| block != AIR) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalidation requires destruction edits",
        ));
    }
    let ids: BTreeSet<_> = edits
        .iter()
        .filter_map(|&(x, y, z, _)| state.entities.anchored_at(EntityCell::new(x, y, z)))
        .collect();
    if ids.is_empty() {
        return Ok(None);
    }
    let mut coords: BTreeMap<_, _> = edits.iter().map(|&(x, y, z, b)| ((x, y, z), b)).collect();
    let mut batches = Vec::new();
    let mut drops = Vec::new();
    for id in ids {
        let snapshot = state
            .entities
            .snapshot(id)
            .ok_or_else(|| io::Error::other("missing invalidated entity"))?;
        let (cells, stacks) = removal(state, &snapshot)?;
        for (c, expected) in cells {
            if state.entities.anchored_at(c) != Some(id)
                || cached_block_or_request(
                    state,
                    c.x,
                    c.y,
                    c.z,
                    "invalidation footprint unavailable",
                )? != expected
            {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "invalidation footprint changed",
                ));
            }
            coords.insert((c.x, c.y, c.z), AIR);
        }
        let a = snapshot.anchor().unwrap();
        // Preflight the complete merge neighborhood before the shared drop
        // planner queries/collects candidates. Its normal dependency capture
        // subsequently retains these same record/motion/absence preimages.
        if !stacks.is_empty() {
            state
                .entities
                .capture_mobile_dependencies(
                    [a.x as f32 + 0.5, a.y as f32 + 0.5, a.z as f32 + 0.5],
                    1.0,
                )
                .map_err(capacity_error)?;
        }
        drops.extend(stacks.into_iter().map(|s| {
            (
                [a.x as f32 + 0.5, a.y as f32 + 0.5, a.z as f32 + 0.5],
                s,
                Duration::from_millis(250),
            )
        }));
        batches.push(
            state
                .entities
                .prepare_despawn(id, snapshot.revision)
                .map_err(capacity_error)?,
        );
    }
    if let Some(batch) = crate::server::drops::plan_stack_spawns(
        &state.entities,
        state.world.catalog(),
        &drops,
        tick,
        crate::server::drops::unix_ms(),
    )
    .map_err(|error| {
        if let Some(cause) = error
            .get_ref()
            .and_then(|e| e.downcast_ref::<entities::EntityError>())
        {
            capacity_error(cause.clone())
        } else {
            error
        }
    })? {
        batches.push(batch);
    }
    let entities = state
        .entities
        .combine_prepared(batches)
        .map_err(capacity_error)?;
    let coords: Vec<_> = coords
        .into_iter()
        .map(|((x, y, z), b)| (x, y, z, b))
        .collect();
    let world_edits = state.world.prepare_edits(&coords)?;
    let deltas = prepared_deltas(&coords, &world_edits);
    Ok(Some(CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits,
        deltas,
        changed_cells: coords
            .iter()
            .map(|&(x, y, z, _)| CellCoord::new(x, y, z))
            .collect(),
        pickups: vec![],
        fire_seed: None,
        entity_wakes: vec![],
        entities: Some(entities),
    }))
}

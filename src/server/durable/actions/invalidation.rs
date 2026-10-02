//! Expand system destruction into complete registered-footprint removal. This is
//! transaction assembly, not a second simulation or fire behavior implementation.
use super::*;
use crate::inventory::Stack;
use crate::server::entities::{self, CellCoord as EntityCell, EntityLocation, EntitySnapshot};
use std::collections::BTreeMap;
type RemovalParts = (Vec<(EntityCell, BlockId)>, Vec<Stack>);

pub(in crate::server) mod gameplay;

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
    catalog: &std::sync::Arc<crate::content::Catalog>,
    lifecycles: &crate::server::lifecycle::Registry,
    snapshot: &EntitySnapshot,
    touched: Option<EntityCell>,
) -> io::Result<RemovalParts> {
    Ok((
        removal_cells(catalog, lifecycles, snapshot, touched)?,
        removal_refunds(catalog, lifecycles, snapshot)?,
    ))
}

fn removal_cells(
    catalog: &std::sync::Arc<crate::content::Catalog>,
    lifecycles: &crate::server::lifecycle::Registry,
    snapshot: &EntitySnapshot,
    touched: Option<EntityCell>,
) -> io::Result<Vec<(EntityCell, BlockId)>> {
    let anchor = snapshot
        .anchor()
        .ok_or_else(|| io::Error::other("not anchored"))?;
    if let Some(d) = catalog.anchored_entity(snapshot.entity_type) {
        let adapter = entities::anchored::Adapter {
            definition: d.clone(),
            catalog: catalog.clone(),
        };
        return adapter.cells(anchor).map_err(io::Error::other);
    }
    if let Some(d) = catalog.machine(snapshot.entity_type) {
        let adapter = entities::machine::Adapter::new(catalog.clone(), d.clone());
        let p = snapshot
            .private_payload
            .downcast_ref::<entities::machine::MachinePayload>()
            .ok_or_else(|| io::Error::other("invalid machine payload"))?;
        return adapter.cells(anchor, p).map_err(io::Error::other);
    }
    let EntityLocation::Anchored { anchor_state, .. } = snapshot.location else {
        unreachable!()
    };
    let d = lifecycles
        .for_state(catalog, anchor_state)
        .ok_or_else(|| io::Error::other("unregistered anchored invalidation"))?;
    if d.entity != snapshot.entity_type {
        return Err(io::Error::other("wrong lifecycle entity"));
    }
    let cells = if let Some(touched) = touched {
        let EntityLocation::Anchored { ref footprint, .. } = snapshot.location else {
            unreachable!()
        };
        let coordinates: Vec<_> = footprint.iter().map(|c| [c.x, c.y, c.z]).collect();
        d.definition
            .plan_remove(bloxgloom_host_api::lifecycle::RemovalContext {
                anchor: [anchor.x, anchor.y, anchor.z],
                touched: [touched.x, touched.y, touched.z],
                footprint: &coordinates,
            })
            .map_err(io::Error::other)?
            .cells
    } else {
        d.definition
            .plan_place(bloxgloom_host_api::lifecycle::PlacementContext {
                anchor: [anchor.x, anchor.y, anchor.z],
                state: &d.definition.anchor_state,
            })
            .map_err(io::Error::other)?
            .cells
    };
    Ok(cells
        .into_iter()
        .zip(&d.states)
        .map(|((c, _), s)| (EntityCell::new(c[0], c[1], c[2]), *s))
        .collect())
}

fn removal_refunds(
    catalog: &std::sync::Arc<crate::content::Catalog>,
    lifecycles: &crate::server::lifecycle::Registry,
    snapshot: &EntitySnapshot,
) -> io::Result<Vec<Stack>> {
    if let Some(d) = catalog.anchored_entity(snapshot.entity_type) {
        return super::anchored::refund_stacks(
            catalog,
            d,
            &snapshot.private_payload,
            bloxgloom_host_api::anchored::RemovalCause::WorldEdit,
        );
    }
    if let Some(d) = catalog.machine(snapshot.entity_type) {
        let p = snapshot
            .private_payload
            .downcast_ref::<entities::machine::MachinePayload>()
            .ok_or_else(|| io::Error::other("invalid machine payload"))?;
        let item = catalog.items().find(|i| i.key == d.item).unwrap().id;
        let mut drops = vec![Stack::new(item, 1)];
        drops.extend(p.slots.iter().flatten().cloned());
        return Ok(drops);
    }
    let EntityLocation::Anchored { anchor_state, .. } = snapshot.location else {
        return Err(io::Error::other("not anchored"));
    };
    let d = lifecycles
        .for_state(catalog, anchor_state)
        .filter(|d| d.entity == snapshot.entity_type)
        .ok_or_else(|| io::Error::other("unregistered anchored refund"))?;
    let p = snapshot
        .private_payload
        .downcast_ref::<entities::container::ContainerPayload>()
        .ok_or_else(|| io::Error::other("invalid storage payload"))?;
    let mut drops = vec![Stack::new(d.item, 1)];
    drops.extend(p.slots.iter().flatten().cloned());
    Ok(drops)
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
        let (cells, stacks) = removal(
            &state.world.catalog_arc(),
            &state.lifecycles,
            &snapshot,
            None,
        )?;
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
    let coords: Vec<_> = coords
        .into_iter()
        .map(|((x, y, z), b)| (x, y, z, b))
        .collect();
    let catalog = state.world.catalog_arc();
    let original = coords
        .iter()
        .map(|&(x, y, z, _)| [x, y, z])
        .collect::<BTreeSet<_>>();
    let mut removals = Vec::with_capacity(edits.len());
    for &(x, y, z, _) in edits {
        let previous = cached_block_or_request(state, x, y, z, "burn target unavailable")?;
        removals.push((previous, [x, y, z], RemovalCause::Burn));
    }
    let mut reads = TerrainReads::default();
    let mut requested = Vec::new();
    let planned = crate::server::gameplay::plan_removals(
        &mut state.world,
        &mut reads,
        &mut requested,
        crate::server::gameplay::OperationInput {
            edits: &coords,
            removals: &removals,
            seed: state.seed,
            tick,
            action: None,
        },
        crate::server::gameplay::Participants {
            actor_inventory_revision: None,
            profile_inventories: None,
            profile_services: None,
            players: &[],
            action_id: None,
            clock: Some(state.world_time.capture()),
            weather: Some(state.weather.capture()),
            actor: None,
            actor_position: None,
            admin: false,
            entities: &state.entities,
        },
    );
    for key in requested {
        let _ = request_chunk(state, key);
    }
    let planned = planned?;
    if coords.iter().any(|edit| !planned.edits.contains(edit)) {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "burn handler rewrote an invalidated footprint",
        ));
    }
    for &(x, y, z, block) in &planned.edits {
        if !original.contains(&[x, y, z]) {
            ensure_no_unhandled_anchor(state, &[(x, y, z, block)])?;
        }
        if catalog.block_flags(block) & crate::content::SOLID != 0
            && state.clients.values().any(|client| {
                block_intersects_player(
                    catalog
                        .player_rules()
                        .for_stance(client.movement.crouching())
                        .body(),
                    [x, y, z],
                    client.position(),
                )
            })
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "burn effect overlaps a player",
            ));
        }
    }
    drops.extend(planned.drops);
    if let Some(batch) = crate::server::drops::plan_stack_spawns_with_extra(
        &state.entities,
        &catalog,
        &drops,
        planned.entity_spawns,
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
    batches.extend(planned.entity_updates);
    let entities = state
        .entities
        .combine_prepared(batches)
        .map_err(capacity_error)?;
    let deltas = prepared_deltas(&planned.edits, &planned.prepared);
    Ok(Some(CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: reads,
        inventory_before: None,
        inventory: None,
        world_edits: planned.prepared,
        deltas,
        changed_cells: planned
            .edits
            .iter()
            .map(|&(x, y, z, _)| CellCoord::new(x, y, z))
            .collect(),
        pickups: vec![],
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entity_wakes: vec![],
        owner_changes: vec![],
        sounds: planned.sounds,
        player_publication: None,
        entities: Some(entities),
    }))
}

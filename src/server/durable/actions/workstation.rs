//! Trusted anchored-workstation edit planning. No world, item, or entity mutation is
//! visible here; all participants enter one WAL record through `CommitAction`.

use super::entity::corrupt;
use super::{BlockEditCommand, prepared_deltas};
use crate::content::{PLANT, REPLACEABLE, SOLID};
use crate::inventory::{HOTBAR_SLOTS, InventoryStore, Stack};
use crate::server::block_actions::{BlockActionContext, BlockCommitBuilder};
use crate::server::durable::CommitAction;
use crate::server::effects::CellCoord as EffectCell;
use crate::server::entities::CellCoord;
use crate::server::simulation::TickId;
use crate::server::{AIR, block_intersects_player};
use crate::world::BlockId;
use std::io::{self, ErrorKind};
use std::time::Duration;

pub(super) struct Placement {
    pub item: crate::items::ItemId,
    pub cost: u16,
    pub cells: Vec<(CellCoord, BlockId)>,
    pub spawn: crate::server::entities::EntitySpawn,
}

pub(super) fn place(
    context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    tick: TickId,
    command: BlockEditCommand,
    previous: BlockId,
    plan: Placement,
) -> io::Result<CommitAction> {
    let catalog = context.catalog();
    let Placement {
        item,
        cost,
        cells,
        spawn,
    } = plan;
    let inventory_before = {
        let client = context.client(command.id).ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotConnected,
                "workstation placement client disconnected",
            )
        })?;
        client.inventory.clone()
    };
    let selected = inventory_before
        .slots
        .get(command.slot as usize)
        .and_then(Option::as_ref)
        .filter(|stack| {
            usize::from(command.slot) < HOTBAR_SLOTS
                && stack.item == item
                && stack.components.is_none()
        })
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::PermissionDenied,
                "selected workstation item mismatch",
            )
        })?;
    if cost == 0 || selected.count < cost {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "selected workstation stack empty",
        ));
    }
    let anchor = CellCoord::new(command.x, command.y, command.z);
    let mut coords = Vec::with_capacity(cells.len());
    let mut displaced_plants = Vec::new();
    for (cell, block) in &cells {
        if cell.y <= crate::world::BEDROCK_Y {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "footprint crosses bedrock",
            ));
        }
        let before = if *cell == anchor {
            previous
        } else {
            builder.cached_block_or_request(
                cell.x,
                cell.y,
                cell.z,
                "workstation footprint chunk is not resident",
            )?
        };
        if catalog.block_flags(before) & REPLACEABLE == 0
            || context.entities().anchored_at(*cell).is_some()
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "workstation footprint cannot be replaced",
            ));
        }
        if catalog.block_flags(*block) & SOLID != 0
            && context.clients().values().any(|client| {
                block_intersects_player(
                    catalog
                        .player_rules()
                        .for_stance(client.movement.crouching())
                        .body(),
                    [cell.x, cell.y, cell.z],
                    client.position(),
                )
            })
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "workstation footprint overlaps a player",
            ));
        }
        coords.push((cell.x, cell.y, cell.z, *block));
        if catalog.block_flags(before) & PLANT != 0 {
            displaced_plants.push((before, [cell.x, cell.y, cell.z]));
        }
    }
    let mut inventory = inventory_before.clone();
    for _ in 0..cost {
        if !inventory.consume(command.slot, item) {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "selected workstation stack empty",
            ));
        }
    }
    let removals = displaced_plants
        .into_iter()
        .map(|(id, at)| {
            (
                id,
                at,
                bloxgloom_host_api::gameplay::RemovalCause::Replacement,
            )
        })
        .collect::<Vec<_>>();
    let plan = builder.plan_removals(
        &coords,
        &removals,
        context.seed(),
        tick.get(),
        crate::server::gameplay::Participants {
            actor_inventory_revision: Some(inventory_before.revision),
            profile_inventories: None,
            profile_services: None,
            player_modifiers: None,
            players: &[],
            action_id: None,
            clock: Some(context.clock()),
            weather: Some(context.weather()),
            actor: Some((command.profile, &inventory)),
            actor_position: context.client(command.id).map(|client| client.position()),
            admin: false,
            entities: context.entities(),
        },
    )?;
    let inventory = plan.inventory.unwrap_or(inventory);
    for &(x, y, z, block) in &plan.edits {
        if context
            .entities()
            .anchored_at(CellCoord::new(x, y, z))
            .is_some()
            || (catalog.block_flags(block) & SOLID != 0
                && context.clients().values().any(|client| {
                    block_intersects_player(
                        catalog
                            .player_rules()
                            .for_stance(client.movement.crouching())
                            .body(),
                        [x, y, z],
                        client.position(),
                    )
                }))
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "gameplay edit conflicts with an anchor or player",
            ));
        }
    }
    // The creation footprint is a lifecycle invariant, not author-owned bytes.
    if coords.iter().any(|edit| !plan.edits.contains(edit)) {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "gameplay handler changed the new anchored footprint",
        ));
    }
    let coords = plan.edits;
    let world_edits = plan.prepared;
    let deltas = prepared_deltas(&coords, &world_edits);
    // The kiln spawn rides in the same atomic batch as its displaced-plant
    // drops: one WAL record, so the kiln and its loot never split.
    let mut extra_spawns = plan.entity_spawns;
    extra_spawns.push(spawn);
    let entities = crate::server::drops::plan_stack_spawns_with_extra(
        context.entities(),
        &catalog,
        &plan.drops,
        extra_spawns,
        tick.get(),
        crate::server::drops::unix_ms(),
    )?
    .ok_or_else(|| io::Error::other("workstation placement planned no work"))?;
    let entities = crate::server::gameplay::combine_entities(
        context.entities(),
        Some(entities),
        plan.entity_updates,
    )?
    .expect("workstation placement contains a spawn");
    Ok(CommitAction {
        client_id: Some(command.id),
        profile: Some(command.profile),
        action_id: Some(command.action_id),
        receipt_value: Some(command.receipt_value),
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: Some(InventoryStore::encode_snapshot_with_catalog(
            &inventory_before,
            &catalog,
        )?),
        inventory: Some(inventory),
        world_edits,
        deltas,
        changed_cells: coords
            .iter()
            .map(|&(x, y, z, _)| EffectCell::new(x, y, z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        sounds: Vec::new(),
        player_publication: None,
        entities: Some(entities),
    })
}

pub(super) struct Removal {
    pub snapshot: crate::server::entities::EntitySnapshot,
    pub cells: Vec<(CellCoord, BlockId)>,
    pub drops: Vec<Stack>,
}

pub(super) fn remove(
    context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    tick: TickId,
    command: BlockEditCommand,
    plan: Removal,
) -> io::Result<CommitAction> {
    let catalog = context.catalog();
    let Removal {
        snapshot,
        cells,
        drops,
    } = plan;
    let id = snapshot.id;
    let anchor = snapshot.anchor().ok_or_else(|| corrupt("not anchored"))?;
    let mut coords = Vec::with_capacity(cells.len());
    for (cell, expected) in &cells {
        if context.entities().anchored_at(*cell) != Some(id) {
            return Err(corrupt("workstation footprint index is incomplete"));
        }
        let actual = builder.cached_block_or_request(
            cell.x,
            cell.y,
            cell.z,
            "workstation footprint chunk is not resident",
        )?;
        if actual != *expected {
            return Err(corrupt(
                "workstation block state differs from anchored entity",
            ));
        }
        coords.push((cell.x, cell.y, cell.z, AIR));
    }
    let inventory_before = context
        .client(command.id)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "missing actor"))?
        .inventory
        .clone();
    let target = cells
        .iter()
        .find(|(cell, _)| *cell == anchor)
        .ok_or_else(|| corrupt("anchor absent from footprint"))?
        .1;
    let plan = builder.plan_removals(
        &coords,
        &[(
            target,
            [anchor.x, anchor.y, anchor.z],
            bloxgloom_host_api::gameplay::RemovalCause::AnchoredBreak,
        )],
        context.seed(),
        tick.get(),
        crate::server::gameplay::Participants {
            actor_inventory_revision: None,
            profile_inventories: None,
            profile_services: None,
            player_modifiers: None,
            players: &[],
            action_id: None,
            clock: Some(context.clock()),
            weather: Some(context.weather()),
            actor: Some((command.profile, &inventory_before)),
            actor_position: context.client(command.id).map(|client| client.position()),
            admin: false,
            entities: context.entities(),
        },
    )?;
    if coords.iter().any(|edit| !plan.edits.contains(edit)) {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "gameplay handler changed the removed footprint",
        ));
    }
    for &(x, y, z, block) in &plan.edits {
        let part_of_footprint = coords
            .iter()
            .any(|&(cx, cy, cz, _)| [x, y, z] == [cx, cy, cz]);
        if !part_of_footprint
            && (context
                .entities()
                .anchored_at(CellCoord::new(x, y, z))
                .is_some()
                || (catalog.block_flags(block) & crate::content::SOLID != 0
                    && context.clients().values().any(|client| {
                        block_intersects_player(
                            catalog
                                .player_rules()
                                .for_stance(client.movement.crouching())
                                .body(),
                            [x, y, z],
                            client.position(),
                        )
                    })))
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "gameplay edit conflicts with an anchor or player",
            ));
        }
    }
    let entities = context
        .entities()
        .prepare_despawn(id, snapshot.revision)
        .map_err(io::Error::other)?;
    let coords = plan.edits;
    let world_edits = plan.prepared;
    let deltas = prepared_deltas(&coords, &world_edits);
    let drop_position = [
        anchor.x as f32 + 0.5,
        anchor.y as f32 + 0.5,
        anchor.z as f32 + 0.5,
    ];
    let mut spawns: Vec<_> = drops
        .into_iter()
        .map(|stack: Stack| (drop_position, stack, Duration::from_millis(250)))
        .collect();
    spawns.extend(plan.drops);
    // The kiln despawn and its refund drops stage as one atomic batch.
    let entities = match crate::server::drops::plan_stack_spawns_with_extra(
        context.entities(),
        &catalog,
        &spawns,
        plan.entity_spawns,
        tick.get(),
        crate::server::drops::unix_ms(),
    )? {
        Some(drops) => context
            .entities()
            .combine_prepared(vec![entities, drops])
            .map_err(io::Error::other)?,
        None => entities,
    };
    let entities = crate::server::gameplay::combine_entities(
        context.entities(),
        Some(entities),
        plan.entity_updates,
    )?
    .expect("anchored removal includes a despawn");
    Ok(CommitAction {
        client_id: Some(command.id),
        profile: Some(command.profile),
        action_id: Some(command.action_id),
        receipt_value: Some(command.receipt_value),
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: plan
            .inventory
            .as_ref()
            .map(|_| InventoryStore::encode_snapshot_with_catalog(&inventory_before, &catalog))
            .transpose()?,
        inventory: plan.inventory,
        world_edits,
        deltas,
        changed_cells: coords
            .iter()
            .map(|&(x, y, z, _)| EffectCell::new(x, y, z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        sounds: Vec::new(),
        player_publication: None,
        entities: Some(entities),
    })
}

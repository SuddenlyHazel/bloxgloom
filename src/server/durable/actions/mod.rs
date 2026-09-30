//! Planning for durable gameplay commands and their exact WAL participants.

use super::*;
use crate::server::block_actions;
use crate::server::streaming::request_chunk;
use crate::server::{AIR, BEDROCK_Y, EDIT_REACH, State, block_intersects_player, world_to_chunk};
use crate::world::BlockId;
use bloxgloom_host_api::gameplay::RemovalCause;

mod admin;
pub(in crate::server) mod anchored;
pub(in crate::server) mod entity;
mod gameplay_action;
pub(in crate::server) mod gameplay_fire;
mod gameplay_pickup;
pub(in crate::server) mod gameplay_tick;
pub(in crate::server) mod invalidation;
pub(in crate::server) mod machine;
mod mobile_lifecycle;
mod registered;
pub(in crate::server) mod storage_lifecycle;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
pub(in crate::server) mod workstation;

pub(in crate::server) fn plan_durable_request(
    state: &mut State,
    request: &DurableRequest,
    tick: TickId,
) -> io::Result<Option<CommitAction>> {
    match request {
        DurableRequest::Command { id, message, .. } => {
            let Some(client) = state.clients.get(id) else {
                return Ok(None);
            };
            let profile = client.profile;
            let action_id = match message {
                ClientMessage::SetWorldTime { action_id, .. }
                | ClientMessage::Edit { action_id, .. }
                | ClientMessage::InventoryMove { action_id, .. }
                | ClientMessage::DropStack { action_id, .. }
                | ClientMessage::AdminGive { action_id, .. }
                | ClientMessage::AdminSpawnEntity { action_id, .. }
                | ClientMessage::EntityInteract { action_id, .. } => *action_id,
                _ => {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "not a durable action",
                    ));
                }
            };
            if action_id == 0 {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "action ID must be nonzero",
                ));
            }
            let receipt_value =
                super::state::encode_action_receipt_with_catalog(message, state.world.catalog())?;
            if state.durability.profile_reserved(profile) {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "profile has a pending durable action",
                ));
            }
            let inventory_revision = state.clients[id].inventory.revision;
            // Keep the wire receipt identity, but resolve stock commands through
            // the same frozen action registry and gameplay context as packages.
            // Only edits retain their separate host reach/placement validation.
            match message {
                ClientMessage::SetWorldTime { elapsed_ms, .. } => registered::plan_request(
                    state,
                    *id,
                    profile,
                    action_id,
                    [0; 3],
                    bloxgloom_host_api::actions::Request {
                        key: crate::gameplay::admin::TIME.into(),
                        version: 1,
                        slot: 0,
                        inventory_revision,
                        entity: 0,
                        entity_revision: 0,
                        arguments: elapsed_ms.to_le_bytes().to_vec(),
                    },
                    receipt_value,
                    tick,
                )
                .map(Some),
                ClientMessage::AdminSpawnEntity { entity_type, .. } => {
                    let key = state
                        .world
                        .catalog()
                        .entity_type(*entity_type)
                        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "unknown creature"))?
                        .key
                        .to_string();
                    let arguments = registered::encode_command_arguments(
                        state.world.catalog(),
                        crate::gameplay::admin::SPAWN,
                        &[&key],
                    )?;
                    registered::plan_request(
                        state,
                        *id,
                        profile,
                        action_id,
                        [0; 3],
                        bloxgloom_host_api::actions::Request {
                            key: crate::gameplay::admin::SPAWN.into(),
                            version: 1,
                            slot: 0,
                            inventory_revision,
                            entity: 0,
                            entity_revision: 0,
                            arguments,
                        },
                        receipt_value,
                        tick,
                    )
                    .map(Some)
                }
                ClientMessage::AdminGive { item, count, .. } => {
                    let key = state
                        .world
                        .catalog()
                        .item(*item)
                        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "unknown item"))?
                        .key
                        .to_string();
                    let arguments = registered::encode_command_arguments(
                        state.world.catalog(),
                        crate::gameplay::admin::GIVE,
                        &[&key, &count.to_string()],
                    )?;
                    registered::plan_request(
                        state,
                        *id,
                        profile,
                        action_id,
                        [0; 3],
                        bloxgloom_host_api::actions::Request {
                            key: crate::gameplay::admin::GIVE.into(),
                            version: 1,
                            slot: 0,
                            inventory_revision,
                            entity: 0,
                            entity_revision: 0,
                            arguments,
                        },
                        receipt_value,
                        tick,
                    )
                    .map(Some)
                }
                ClientMessage::InventoryMove {
                    from, to, count, ..
                } => {
                    let mut arguments = vec![*from, *to];
                    arguments.extend(count.to_le_bytes());
                    registered::plan_request(
                        state,
                        *id,
                        profile,
                        action_id,
                        [0; 3],
                        bloxgloom_host_api::actions::Request {
                            key: crate::gameplay::slot_move::KEY.into(),
                            version: 1,
                            slot: 0,
                            inventory_revision,
                            entity: 0,
                            entity_revision: 0,
                            arguments,
                        },
                        receipt_value,
                        tick,
                    )
                    .map(Some)
                }
                ClientMessage::DropStack { slot, count, .. } => {
                    let mut arguments = vec![*slot];
                    arguments.extend(count.to_le_bytes());
                    registered::plan_request(
                        state,
                        *id,
                        profile,
                        action_id,
                        [0; 3],
                        bloxgloom_host_api::actions::Request {
                            key: crate::gameplay::drop_stack::KEY.into(),
                            version: 1,
                            slot: *slot,
                            inventory_revision,
                            entity: 0,
                            entity_revision: 0,
                            arguments,
                        },
                        receipt_value,
                        tick,
                    )
                    .map(Some)
                }
                ClientMessage::Edit {
                    x,
                    y,
                    z,
                    block,
                    slot,
                    ..
                } => plan_block_edit(
                    state,
                    tick,
                    BlockEditCommand {
                        id: *id,
                        profile,
                        action_id,
                        receipt_value,
                        x: *x,
                        y: *y,
                        z: *z,
                        block: *block,
                        slot: *slot,
                    },
                )
                .map(Some),
                ClientMessage::EntityInteract {
                    action_id,
                    target,
                    payload,
                } => registered::plan(
                    state,
                    *id,
                    profile,
                    *action_id,
                    *target,
                    payload,
                    receipt_value,
                    tick,
                )
                .map(Some),
                _ => unreachable!(),
            }
        }
        DurableRequest::Pickup { id } => gameplay_pickup::plan(state, *id, tick.get()),
        DurableRequest::Expire => {
            let Some(entities) = crate::server::drops::plan_expired(
                &state.entities,
                state.world.catalog(),
                crate::server::drops::unix_ms(),
                256,
            )?
            else {
                state.durability.expire_queued = false;
                state.durability.expire_again = false;
                return Ok(None);
            };
            Ok(Some(CommitAction {
                client_id: None,
                profile: None,
                action_id: None,
                receipt_value: None,
                receipt_transition: None,
                terrain_reads: Default::default(),
                inventory_before: None,
                inventory: None,
                world_edits: Vec::new(),
                deltas: Vec::new(),
                changed_cells: Vec::new(),
                pickups: Vec::new(),
                fire_seed: None,
                clock_change: None,
                entity_wakes: Vec::new(),
                entities: Some(entities),
            }))
        }
        DurableRequest::EntityTick { id } => {
            if gameplay_tick::is_registered(state, *id) {
                gameplay_tick::plan(state, *id, tick.get(), false)
            } else {
                entity::plan_entity_tick(state, *id, tick.get(), false)
            }
        }
        DurableRequest::EntityWake { id } => {
            if gameplay_tick::is_registered(state, *id) {
                gameplay_tick::plan(state, *id, tick.get(), true)
            } else {
                entity::plan_entity_tick(state, *id, tick.get(), true)
            }
        }
    }
}

pub(in crate::server) struct BlockEditCommand {
    pub(in crate::server) id: u64,
    pub(in crate::server) profile: u128,
    pub(in crate::server) action_id: u128,
    pub(in crate::server) receipt_value: Vec<u8>,
    pub(in crate::server) x: i32,
    pub(in crate::server) y: i32,
    pub(in crate::server) z: i32,
    pub(in crate::server) block: BlockId,
    pub(in crate::server) slot: u8,
}

fn plan_block_edit(
    state: &mut State,
    tick: TickId,
    command: BlockEditCommand,
) -> io::Result<CommitAction> {
    let (id, x, y, z, block) = (command.id, command.x, command.y, command.z, command.block);
    let client = state.clients.get(&id).expect("command client exists");
    let position = client.position();
    let inventory_before = client.inventory.clone();
    let catalog = state.world.catalog_arc();
    let has = |state_id: BlockId, flag: u8| catalog.block_flags(state_id) & flag != 0;
    if catalog.state(block).is_none() || y <= BEDROCK_Y {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalid block edit",
        ));
    }
    let distance_sq = (x as f32 + 0.5 - position[0]).powi(2)
        + (y as f32 + 0.5 - (position[1] + catalog.player_rules().eye_height())).powi(2)
        + (z as f32 + 0.5 - position[2]).powi(2);
    if distance_sq > EDIT_REACH * EDIT_REACH || !client.interested(world_to_chunk(x, y, z).0) {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "block out of reach",
        ));
    }
    let previous = cached_block_or_request(state, x, y, z, "edit target chunk is not resident")?;
    if previous == block {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "block is unchanged",
        ));
    }
    let hooks = state
        .block_actions
        .for_state(&catalog, if block == AIR { previous } else { block });
    if let Some(hooks) = hooks {
        let hook = if block == AIR {
            hooks.break_block
        } else {
            hooks.place
        };
        return block_actions::invoke_hook(hook, state, tick, command, previous);
    }
    let BlockEditCommand {
        profile,
        action_id,
        receipt_value,
        slot,
        ..
    } = command;
    let coords = vec![(x, y, z, block)];
    let mut terrain_reads = TerrainReads::default();
    let mut removed_plants = Vec::new();
    if block != AIR {
        if !has(previous, crate::content::REPLACEABLE) {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "target cannot be replaced",
            ));
        }
        if has(block, crate::content::PLANT) {
            if y == i32::MIN {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "plant needs soil below",
                ));
            }
            if !has(
                cached_block_with_reads(
                    state,
                    &mut terrain_reads,
                    x,
                    y - 1,
                    z,
                    "plant support chunk is not resident",
                )?,
                crate::content::SUPPORTS_PLANT,
            ) {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "plant needs soil below",
                ));
            }
        }
        if has(block, crate::content::SOLID)
            && state.clients.values().any(|other| {
                block_intersects_player(catalog.player_rules().body(), [x, y, z], other.position())
            })
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "block overlaps a player",
            ));
        }
        let selected = inventory_before
            .slots
            .get(slot as usize)
            .cloned()
            .flatten()
            .filter(|stack| {
                (slot as usize) < crate::inventory::HOTBAR_SLOTS
                    && crate::items::placeable_block_in(stack.item, &catalog) == Some(block)
            })
            .ok_or_else(|| {
                io::Error::new(ErrorKind::PermissionDenied, "selected stack mismatch")
            })?;
        let mut updated = inventory_before.clone();
        if !updated.consume(slot, selected.item) {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "selected stack empty",
            ));
        }
        if has(previous, crate::content::PLANT) {
            removed_plants.push((previous, [x, y, z]));
        }
        ensure_no_unhandled_anchor(state, &coords)?;
        let removals = removed_plants
            .into_iter()
            .map(|(id, at)| (id, at, RemovalCause::Replacement))
            .collect::<Vec<_>>();
        let plan = plan_gameplay_removals(
            state,
            &mut terrain_reads,
            &coords,
            &removals,
            (profile, &updated),
            position,
            tick.get(),
        )?;
        let updated = plan.inventory.unwrap_or(updated);
        let coords = plan.edits;
        let prepared = plan.prepared;
        let deltas = prepared_deltas(&coords, &prepared);
        let entities = crate::server::drops::plan_stack_spawns_with_extra(
            &state.entities,
            &catalog,
            &plan.drops,
            plan.entity_spawns,
            tick.get(),
            crate::server::drops::unix_ms(),
        )?;
        let entities = crate::server::gameplay::combine_entities(
            &state.entities,
            entities,
            plan.entity_updates,
        )?;
        let (source, local) = world_to_chunk(x, y, z);
        let cell = crate::world::Chunk::index(local)
            .and_then(|index| u16::try_from(index).ok())
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "fire seed cell invalid"))?;
        let fire_seed = state
            .fire
            .prepare_seed_from_edit(tick, source, cell, block)?;
        return Ok(CommitAction {
            client_id: Some(id),
            profile: Some(profile),
            action_id: Some(action_id),
            receipt_value: Some(receipt_value),
            receipt_transition: None,
            terrain_reads,
            inventory_before: Some(InventoryStore::encode_snapshot_with_catalog(
                &inventory_before,
                &catalog,
            )?),
            inventory: Some(updated),
            world_edits: prepared,
            deltas,
            changed_cells: coords
                .into_iter()
                .map(|(x, y, z, _)| CellCoord::new(x, y, z))
                .collect(),
            pickups: Vec::new(),
            fire_seed,
            clock_change: None,
            entities,
            entity_wakes: Vec::new(),
        });
    }

    if previous == AIR {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "target is already air",
        ));
    }
    ensure_no_unhandled_anchor(state, &coords)?;
    let removals = vec![(previous, [x, y, z], RemovalCause::Break)];
    let plan = plan_gameplay_removals(
        state,
        &mut terrain_reads,
        &coords,
        &removals,
        (profile, &inventory_before),
        position,
        tick.get(),
    )?;
    let coords = plan.edits;
    let prepared = plan.prepared;
    let deltas = prepared_deltas(&coords, &prepared);
    let entities = crate::server::drops::plan_stack_spawns_with_extra(
        &state.entities,
        &catalog,
        &plan.drops,
        plan.entity_spawns,
        tick.get(),
        crate::server::drops::unix_ms(),
    )?;
    let entities =
        crate::server::gameplay::combine_entities(&state.entities, entities, plan.entity_updates)?;
    Ok(CommitAction {
        client_id: Some(id),
        profile: Some(profile),
        action_id: Some(action_id),
        receipt_value: Some(receipt_value),
        receipt_transition: None,
        terrain_reads,
        inventory_before: plan
            .inventory
            .as_ref()
            .map(|_| InventoryStore::encode_snapshot_with_catalog(&inventory_before, &catalog))
            .transpose()?,
        inventory: plan.inventory,
        world_edits: prepared,
        deltas,
        changed_cells: coords
            .into_iter()
            .map(|(x, y, z, _)| CellCoord::new(x, y, z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        entity_wakes: Vec::new(),
        entities,
    })
}

fn plan_gameplay_removals(
    state: &mut State,
    reads: &mut TerrainReads,
    edits: &[crate::server::gameplay::Edit],
    removals: &[crate::server::gameplay::Removal],
    actor: (u128, &Inventory),
    actor_position: [f32; 3],
    tick: u64,
) -> io::Result<crate::server::gameplay::WorldPlan> {
    let mut requested = Vec::new();
    let result = crate::server::gameplay::plan_removals(
        &mut state.world,
        reads,
        &mut requested,
        crate::server::gameplay::OperationInput {
            edits,
            removals,
            seed: state.seed,
            tick,
            action: None,
        },
        crate::server::gameplay::Participants {
            players: &[],
            action_id: None,
            clock: None,
            actor: Some(actor),
            actor_position: Some(actor_position),
            admin: false,
            entities: &state.entities,
        },
    );
    for key in requested {
        let _ = request_chunk(state, key);
    }
    let plan = result?;
    ensure_no_unhandled_anchor(state, &plan.edits)?;
    let catalog = state.world.catalog();
    for &(x, y, z, block) in &plan.edits {
        if catalog.block_flags(block) & crate::content::SOLID != 0
            && state.clients.values().any(|client| {
                block_intersects_player(catalog.player_rules().body(), [x, y, z], client.position())
            })
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "gameplay block overlaps a player",
            ));
        }
    }
    Ok(plan)
}

/// A failed resident read is not authoritative air. Ask the bounded chunk
/// loader for that exact key before deferring the original command for retry.
/// This matters at vertical interest boundaries, where an edit can depend on
/// a support/plant-check chunk that normal view streaming never requests.
///
/// The load request is best-effort, matching the entity view capture and the
/// block-action builder: a failed request still defers with `WouldBlock` so
/// transient loader pressure never fails work that could simply wait. The
/// cell stays unread, so no preimage is verified and nothing commits on this
/// pass; the coordinator's command timeout is the escape hatch for
/// over-long deferrals.
fn cached_block_with_reads(
    state: &mut State,
    reads: &mut TerrainReads,
    x: i32,
    y: i32,
    z: i32,
    reason: &'static str,
) -> io::Result<BlockId> {
    if let Some(block) = reads.read(&mut state.world, x, y, z)? {
        return Ok(block);
    }
    cached_block_or_request(state, x, y, z, reason)?;
    reads
        .read(&mut state.world, x, y, z)?
        .ok_or_else(|| io::Error::new(ErrorKind::WouldBlock, reason))
}

fn cached_block_or_request(
    state: &mut State,
    x: i32,
    y: i32,
    z: i32,
    reason: &'static str,
) -> io::Result<BlockId> {
    if let Some(block) = state.world.cached_block(x, y, z) {
        return Ok(block);
    }
    let key = world_to_chunk(x, y, z).0;
    // Capacity pressure (a full loader queue, an unrequestable chunk, or a
    // failed request) only defers the command with `WouldBlock` so it can
    // retry later — never commit with an unverified preimage. Only genuine
    // corruption (`InvalidData`) keeps its kind so it still stops the
    // coordinator through the caller's fatal path instead of being
    // conflated with "not available yet".
    match request_chunk(state, key) {
        Ok(_) => Err(io::Error::new(ErrorKind::WouldBlock, reason)),
        Err(error) if error.kind() == ErrorKind::InvalidData => Err(error),
        Err(_) => Err(io::Error::new(ErrorKind::WouldBlock, reason)),
    }
}

fn ensure_no_unhandled_anchor(
    state: &State,
    coords: &[(i32, i32, i32, BlockId)],
) -> io::Result<()> {
    if coords.iter().any(|&(x, y, z, _)| {
        state
            .entities
            .anchored_at(crate::server::entities::CellCoord::new(x, y, z))
            .is_some()
    }) {
        Err(io::Error::new(
            ErrorKind::InvalidData,
            "generic block edit would orphan an anchored entity",
        ))
    } else {
        Ok(())
    }
}

fn prepared_deltas(
    coords: &[(i32, i32, i32, BlockId)],
    prepared: &[PreparedEdit],
) -> Vec<BlockDelta> {
    coords
        .iter()
        .filter_map(|&(x, y, z, block)| {
            let (key, local) = world_to_chunk(x, y, z);
            let version = prepared.iter().find(|edit| edit.key == key)?.new_version;
            Some(BlockDelta {
                key,
                version,
                local: local.map(|value| value as u8),
                block,
            })
        })
        .collect()
}

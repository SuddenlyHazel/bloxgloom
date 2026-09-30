//! Registered actions feed the existing receipt/transaction path. The registry
//! owns effects; client-supplied bytes can only select bounded host operations.
use super::*;
use bloxgloom_host_api::actions::{
    CommandPermission, Operation, Request, TERRAIN_REQUEST_TAG, Target, TerrainRequest,
};

/// Compatibility packets retain their receipt identity but use the same frozen
/// schema and validation as advertised command requests.
pub(super) fn encode_command_arguments(
    catalog: &crate::content::Catalog,
    key: &str,
    values: &[&str],
) -> io::Result<Vec<u8>> {
    catalog
        .action(key)
        .and_then(|action| action.command.as_ref())
        .and_then(|command| command.encode_arguments(values))
        .ok_or_else(|| denied("invalid command arguments"))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan(
    state: &mut State,
    client_id: u64,
    profile: u128,
    action_id: u128,
    target: [i32; 3],
    payload: &[u8],
    receipt_value: Vec<u8>,
    tick: TickId,
) -> io::Result<CommitAction> {
    let catalog = state.world.catalog_arc();
    let mut terrain_version = None;
    let request = if payload.first() == Some(&TERRAIN_REQUEST_TAG) {
        let observed =
            TerrainRequest::decode(payload).ok_or_else(|| denied("malformed terrain action"))?;
        terrain_version = Some(observed.version);
        observed.request
    } else if payload.first() == Some(&bloxgloom_host_api::actions::REQUEST_TAG) {
        Request::decode(payload).ok_or_else(|| denied("malformed registered action"))?
    } else {
        // Existing identity-fenced wire requests resolve through the same frozen
        // registry. Unfenced v1 inventory shortcuts are deliberately rejected.
        let (id, revision, slot, arguments) = match payload {
            [2, direction, slot, player, low, high, rest @ ..] if rest.len() == 16 => (
                u64::from_le_bytes(rest[..8].try_into().unwrap()),
                u64::from_le_bytes(rest[8..].try_into().unwrap()),
                *player,
                vec![*direction, *slot, *low, *high],
            ),
            [3 | 4, rest @ ..] if rest.len() >= 17 => (
                u64::from_le_bytes(rest[..8].try_into().unwrap()),
                u64::from_le_bytes(rest[8..16].try_into().unwrap()),
                0,
                vec![],
            ),
            _ => {
                return Err(denied(
                    "interaction requires identity-fenced registered request",
                ));
            }
        };
        let id_value = crate::server::entities::EntityId::new(id)
            .ok_or_else(|| denied("invalid legacy target"))?;
        let snapshot = state
            .entities
            .snapshot(id_value)
            .ok_or_else(|| denied("legacy target gone"))?;
        let definition = if payload[0] == 2 {
            let screen = catalog.inventory_screen(snapshot.entity_type).ok_or_else(|| denied("no target inventory"))?;
            catalog.action(&format!("{}/inventory",screen.entity))
        } else if payload[0] == 4 {
            let definition = catalog.anchored_entity(snapshot.entity_type).ok_or_else(|| denied("not a registered anchored target"))?;
            catalog.discover_actions(&Target::Block(definition.block.clone()))
                .chain(catalog.discover_actions(&Target::Entity(definition.entity.clone())))
                .find(|a| matches!(&a.operation,Operation::EntityRequest(bytes) if bytes.as_slice() == &payload[17..]))
        } else {
            let key = &catalog.entity_type(snapshot.entity_type).ok_or_else(|| denied("unknown entity"))?.key;
            catalog.discover_actions(&Target::Entity(key.to_string())).find(|a| matches!(&a.operation,Operation::EntityRequest(bytes) if bytes.as_slice() == &payload[17..]))
        }.ok_or_else(|| denied("legacy request has no registered action"))?;
        Request {
            key: definition.key.clone(),
            version: definition.version,
            slot,
            inventory_revision: state
                .clients
                .get(&client_id)
                .ok_or_else(|| denied("actor gone"))?
                .inventory
                .revision,
            entity: id,
            entity_revision: revision,
            arguments,
        }
    };
    plan_observed_request(
        state,
        client_id,
        profile,
        action_id,
        target,
        request,
        terrain_version,
        receipt_value,
        tick,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan_request(
    state: &mut State,
    client_id: u64,
    profile: u128,
    action_id: u128,
    target: [i32; 3],
    request: Request,
    receipt_value: Vec<u8>,
    tick: TickId,
) -> io::Result<CommitAction> {
    plan_observed_request(
        state,
        client_id,
        profile,
        action_id,
        target,
        request,
        None,
        receipt_value,
        tick,
    )
}

#[allow(clippy::too_many_arguments)]
fn plan_observed_request(
    state: &mut State,
    client_id: u64,
    profile: u128,
    action_id: u128,
    target: [i32; 3],
    request: Request,
    terrain_version: Option<u64>,
    receipt_value: Vec<u8>,
    tick: TickId,
) -> io::Result<CommitAction> {
    let catalog = state.world.catalog_arc();
    let action = catalog
        .action(&request.key)
        .ok_or_else(|| denied("unknown registered action"))?;
    if request.version != action.version {
        return Err(denied("unsupported action version"));
    }
    if terrain_version.is_some()
        && !(action.operation == Operation::Gameplay && matches!(action.target, Target::Block(_)))
    {
        return Err(denied("terrain fence requires a gameplay block action"));
    }
    let client = state
        .clients
        .get(&client_id)
        .ok_or_else(|| denied("actor disconnected"))?;
    if let Some(command) = &action.command {
        // Applies to all envelopes/UI entry points for this key. The catalog
        // freezes the permission; profile/admin authority belongs to the server.
        // Retried planning passes these checks again before invoking any handler.
        if profile == 0 || client.profile != profile {
            return Err(denied("command requires an authenticated player"));
        }
        if command.permission == CommandPermission::Admin && state.admin_profile != Some(profile) {
            return Err(denied("command requires admin permission"));
        }
        if catalog
            .command_arguments(command, &request.arguments)
            .is_none()
        {
            return Err(denied("invalid command arguments"));
        }
    }
    // Screen transfers are live-slot intents. Other actions retain the actor's
    // observed inventory fence; the transfer planner captures current slots.
    if action.operation != Operation::Inventory
        && client.inventory.revision != request.inventory_revision
    {
        return Err(denied("stale actor inventory"));
    }
    match &action.operation {
        Operation::Gameplay => gameplay_action::plan(
            state,
            gameplay_action::Invocation {
                client_id,
                profile,
                action_id,
                target,
                request: &request,
                terrain_version,
                kind: &action.target,
                receipt_value,
                tick,
            },
        ),
        Operation::Recipe {
            input,
            consume,
            output: _,
            produce,
        } => {
            if request.entity != 0 || request.entity_revision != 0 || !request.arguments.is_empty()
            {
                return Err(denied("recipe has unexpected target or arguments"));
            }
            let before = &client.inventory;
            let stack = before
                .slots
                .get(usize::from(request.slot))
                .and_then(Option::as_ref)
                .ok_or_else(|| denied("empty recipe input"))?;
            let item = catalog
                .item(stack.item)
                .ok_or_else(|| denied("unknown input"))?;
            if item.key != *input || stack.components.is_some() || stack.count < *consume {
                return Err(denied("recipe requires matching plain input stack"));
            }
            let mut inventory = before.clone();
            inventory.slots[usize::from(request.slot)] = (stack.count > *consume).then(|| {
                let mut remainder = stack.clone();
                remainder.count -= consume;
                remainder
            });
            let output = catalog
                .action_output(&action.key)
                .ok_or_else(|| denied("unknown recipe output"))?;
            if inventory.insert_stack(&crate::inventory::Stack::new(output, *produce), &catalog)
                != 0
            {
                return Err(denied("recipe output does not fit"));
            }
            inventory.revision = before
                .revision
                .checked_add(1)
                .ok_or_else(|| denied("inventory revision exhausted"))?;
            Ok(CommitAction {
                client_id: Some(client_id),
                profile: Some(profile),
                action_id: Some(action_id),
                receipt_value: Some(receipt_value),
                receipt_transition: None,
                terrain_reads: Default::default(),
                inventory_before: Some(InventoryStore::encode_snapshot_with_catalog(
                    before, &catalog,
                )?),
                inventory: Some(inventory),
                world_edits: vec![],
                deltas: vec![],
                changed_cells: vec![],
                pickups: vec![],
                fire_seed: None,
                clock_change: None,
                entities: None,
                entity_wakes: vec![],
            })
        }
        Operation::Inventory | Operation::EntityRequest(_) => {
            let id = crate::server::entities::EntityId::new(request.entity)
                .ok_or_else(|| denied("missing action entity"))?;
            let snapshot = state
                .entities
                .snapshot(id)
                .ok_or_else(|| denied("action entity gone"))?;
            // Mobile use and inventory transfers are intents against current
            // state. The planner still captures and fences the authoritative
            // revision for the WAL commit. Other requests keep client fences.
            let mobile_use = matches!(action.operation, Operation::EntityRequest(_))
                && matches!(action.target, Target::Entity(_))
                && catalog.mobile_entity(snapshot.entity_type).is_some();
            let stale = match action.operation {
                Operation::Inventory => false,
                _ if mobile_use => {
                    request.entity_revision == 0 || request.entity_revision > snapshot.revision
                }
                _ => request.entity_revision == 0 || snapshot.revision != request.entity_revision,
            };
            if stale {
                return Err(denied("stale action entity"));
            }
            let target = if mobile_use {
                let crate::server::entities::EntityLocation::Mobile { position } =
                    snapshot.location
                else {
                    return Err(denied("mobile action location mismatch"));
                };
                position.map(|v| v.floor() as i32)
            } else {
                target
            };
            let kind = catalog
                .entity_type(snapshot.entity_type)
                .ok_or_else(|| denied("unknown entity type"))?;
            let inner = match (&action.target, &action.operation) {
                (Target::Entity(key), Operation::EntityRequest(bytes)) if key == &kind.key => {
                    if !request.arguments.is_empty() {
                        return Err(denied("unexpected entity arguments"));
                    }
                    bytes.clone()
                }
                (Target::Block(key), Operation::EntityRequest(bytes)) => {
                    let crate::server::entities::EntityLocation::Anchored { anchor_state, .. } =
                        snapshot.location
                    else {
                        return Err(denied("block action needs anchored target"));
                    };
                    if catalog
                        .state(anchor_state)
                        .and_then(|s| catalog.block_type(s.block_type))
                        .is_none_or(|b| b.key != *key)
                        || !request.arguments.is_empty()
                    {
                        return Err(denied("block action target mismatch"));
                    }
                    bytes.clone()
                }
                (Target::Block(key), Operation::Inventory) => {
                    let screen = catalog
                        .inventory_screen(snapshot.entity_type)
                        .ok_or_else(|| denied("no inventory screen"))?;
                    if &screen.block != key
                        || state
                            .entities
                            .anchored_at(crate::server::entities::CellCoord::new(
                                target[0], target[1], target[2],
                            ))
                            != Some(id)
                    {
                        return Err(denied("inventory target identity mismatch"));
                    }
                    let [direction, slot, low, high] = request.arguments.as_slice() else {
                        return Err(denied("invalid inventory arguments"));
                    };
                    let mut inner = vec![2, *direction, *slot, request.slot, *low, *high];
                    inner.extend(id.get().to_le_bytes());
                    inner.extend(snapshot.revision.to_le_bytes());
                    inner
                }
                _ => return Err(denied("registered target does not match")),
            };
            let inner = if catalog.anchored_entity(snapshot.entity_type).is_some()
                && matches!(action.operation, Operation::EntityRequest(_))
            {
                bloxgloom_host_api::anchored::interaction_request(
                    id.get(),
                    snapshot.revision,
                    &inner,
                )
                .map_err(|_| denied("invalid anchored action envelope"))?
            } else {
                inner
            };
            entity::plan_interact(
                state,
                client_id,
                profile,
                action_id,
                target,
                &inner,
                receipt_value,
                tick,
                id,
            )
        }
    }
}
fn denied(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::PermissionDenied, message)
}

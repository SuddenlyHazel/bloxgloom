//! Registered actions feed the existing receipt/transaction path. The registry
//! owns effects; client-supplied bytes can only select bounded host operations.
use super::*;
use bloxgloom_host_api::actions::{Operation, Request, Target};

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
    let request = if payload.first() == Some(&bloxgloom_host_api::actions::REQUEST_TAG) {
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
    let action = catalog
        .action(&request.key)
        .ok_or_else(|| denied("unknown registered action"))?;
    if request.version != action.version {
        return Err(denied("unsupported action version"));
    }
    let client = state
        .clients
        .get(&client_id)
        .ok_or_else(|| denied("actor disconnected"))?;
    if client.inventory.revision != request.inventory_revision {
        return Err(denied("stale actor inventory"));
    }
    match &action.operation {
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
                inventory_before: Some(InventoryStore::encode_snapshot_with_catalog(
                    before, &catalog,
                )?),
                inventory: Some(inventory),
                world_edits: vec![],
                deltas: vec![],
                changed_cells: vec![],
                pickups: vec![],
                fire_seed: None,
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
            if snapshot.revision != request.entity_revision {
                return Err(denied("stale action entity"));
            }
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

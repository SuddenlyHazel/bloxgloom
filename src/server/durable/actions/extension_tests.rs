//! Public fixture declarations, production lifecycle dispatch, real WAL recovery.
use super::*;
use crate::inventory::Stack;
use crate::server::entities::container::ContainerPayload;
use crate::server::startup::ServerStartup;
use std::sync::Arc;

fn startup() -> ServerStartup {
    ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&bloxgloom_lifecycle_fixture::TallStore)
        .unwrap()
}
fn open(path: &std::path::Path) -> State {
    crate::server::server_state_with_startup(53, path.to_path_buf(), 8, startup()).unwrap()
}
fn resident(state: &mut State) {
    for y in [79, 80] {
        state.world.get_chunk(world_to_chunk(-1, y, -1).0).unwrap();
    }
}
fn ids(state: &State) -> (BlockId, ItemId) {
    let catalog = state.world.catalog_arc();
    (
        catalog
            .state_by_key(bloxgloom_lifecycle_fixture::KEY)
            .unwrap(),
        catalog
            .items()
            .find(|i| i.key == bloxgloom_lifecycle_fixture::KEY)
            .unwrap()
            .id,
    )
}
fn edit(action_id: u128, y: i32, block: BlockId) -> ClientMessage {
    ClientMessage::Edit {
        action_id,
        x: -1,
        y,
        z: -1,
        block,
        slot: 0,
    }
}

#[test]
fn external_storage_lifecycle_seam_restart_retries_and_exact_refunds() {
    let path = temp_save_dir("external-storage-lifecycle");
    let mut state = open(&path);
    resident(&mut state);
    let (block, item) = ids(&state);
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(item, 2));
    let tagged = Stack::with_components(ItemId(crate::world::STONE.0), 7, 1, vec![12, 34]).unwrap();
    inventory.slots[25] = Some(tagged.clone());
    let peer = add_test_client(&mut state, [-1.0, 79.0, 2.0], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let place = edit(epoch | 1, 79, block);
    settle_live_action(&mut state, 40_000, place.clone());
    settle_live_action(&mut state, 40_001, place);
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        1
    );
    let anchor = CellCoord::new(-1, 79, -1);
    let upper = CellCoord::new(-1, 80, -1);
    let id = state.entities.anchored_at(anchor).unwrap();
    assert_eq!(state.entities.anchored_at(upper), Some(id));
    let snapshot = state.entities.snapshot(id).unwrap();
    let mut payload = vec![2, 0, 8, 25];
    payload.extend(7u16.to_le_bytes());
    payload.extend(id.get().to_le_bytes());
    payload.extend(snapshot.revision.to_le_bytes());
    settle_live_action(
        &mut state,
        40_002,
        ClientMessage::EntityInteract {
            action_id: epoch | 2,
            target: [-1, 80, -1],
            payload,
        },
    );
    assert!(state.clients[&1].inventory.slots[25].is_none());
    assert_eq!(
        state
            .entities
            .snapshot(id)
            .unwrap()
            .private_payload
            .downcast_ref::<ContainerPayload>()
            .unwrap()
            .slots[8],
        Some(tagged.clone())
    );
    drop(peer);
    drop(state);
    let mut state = open(&path);
    resident(&mut state);
    assert!(state.recovered_tick >= 40_002);
    assert_eq!(state.entities.anchored_at(upper), Some(id));
    assert_eq!(
        state
            .entities
            .snapshot(id)
            .unwrap()
            .private_payload
            .downcast_ref::<ContainerPayload>()
            .unwrap()
            .slots[8],
        Some(tagged.clone())
    );
    let inventory = state.inventory_store.load(17).unwrap();
    let peer = add_test_client(&mut state, [-1.0, 79.0, 2.0], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let remove = edit(epoch | 1, 80, AIR);
    settle_live_action(&mut state, 40_010, remove.clone());
    settle_live_action(&mut state, 40_011, remove);
    for cell in [anchor, upper] {
        assert!(state.entities.anchored_at(cell).is_none());
        assert_eq!(state.world.cached_block(cell.x, cell.y, cell.z), Some(AIR));
    }
    let stacks: Vec<_> = crate::server::drops::nearby(&state.entities, [-0.5, 79.5, -0.5])
        .iter()
        .map(|d| {
            crate::server::drops::stack(
                &state.entities,
                crate::server::entities::EntityId::new(d.id).unwrap(),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(
        stacks
            .iter()
            .filter(|s| s.item == item)
            .map(|s| s.count)
            .sum::<u16>(),
        1
    );
    assert_eq!(
        stacks
            .iter()
            .filter(|s| s.item == tagged.item)
            .cloned()
            .collect::<Vec<_>>(),
        vec![tagged]
    );
    drop(peer);
    drop(state);
    let mut state = open(&path);
    resident(&mut state);
    assert!(state.entities.anchored_at(anchor).is_none());
    assert_eq!(state.world.cached_block(-1, 80, -1), Some(AIR));
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn external_storage_rejects_blocked_footprint_and_stale_placement_without_debit() {
    let path = temp_save_dir("external-storage-conflicts");
    let mut state = open(&path);
    resident(&mut state);
    let (block, item) = ids(&state);
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(item, 2));
    let peer = add_test_client(&mut state, [-1.0, 79.0, 2.0], inventory.clone());
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    state.world.edit(-1, 80, -1, crate::world::STONE).unwrap();
    let request = edit_request(edit(epoch | 1, 79, block));
    assert!(plan_durable_request(&mut state, &request, TickId::new(10)).is_err());
    assert_eq!(state.clients[&1].inventory, inventory);
    assert!(
        state
            .entities
            .anchored_at(CellCoord::new(-1, 79, -1))
            .is_none()
    );
    state.world.edit(-1, 80, -1, AIR).unwrap();
    let mut action = plan_durable_request(&mut state, &request, TickId::new(11))
        .unwrap()
        .unwrap();
    let mut competing = plan_durable_request(
        &mut state,
        &edit_request(edit(epoch | 2, 79, block)),
        TickId::new(11),
    )
    .unwrap()
    .unwrap();
    // Direct staging isolates write-key admission; live retry receipts are
    // covered by the preceding production-command test.
    for plan in [&mut action, &mut competing] {
        plan.action_id = None;
        plan.receipt_value = None;
    }
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    assert!(
        state
            .durability
            .try_stage(TickId::new(11), &competing, Some(permit))
            .unwrap()
    );
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    assert!(
        state
            .durability
            .try_stage(TickId::new(11), &action, Some(permit))
            .is_err()
    );
    assert_eq!(state.clients[&1].inventory, inventory);
    assert!(
        state
            .entities
            .anchored_at(CellCoord::new(-1, 79, -1))
            .is_none()
    );
    assert_eq!(state.durability.pending.len(), 1);
    crate::server::durable::complete_barrier(
        &mut state,
        crate::server::durable::CommitBarrier::AllStaged,
    )
    .unwrap();
    assert!(
        state
            .entities
            .anchored_at(CellCoord::new(-1, 79, -1))
            .is_some()
    );
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

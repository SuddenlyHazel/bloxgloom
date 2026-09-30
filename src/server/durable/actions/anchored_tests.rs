use super::*;
use crate::{
    inventory::Stack,
    server::{entities::*, startup::ServerStartup},
};
use std::sync::Arc;
#[path = "reaction_removal_tests.rs"]
mod reaction_removal_tests;
const KEY: &str = bloxgloom_lifecycle_fixture::anchored::KEY;
fn open(path: &std::path::Path) -> State {
    let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&bloxgloom_lifecycle_fixture::anchored::SignalPost)
        .unwrap();
    crate::server::server_state_with_startup(53, path.to_path_buf(), 8, startup).unwrap()
}
fn resident(state: &mut State) {
    for x in -2..=0 {
        for y in 3..=5 {
            for z in -2..=0 {
                state
                    .world
                    .get_chunk(crate::world::ChunkKey { x, y, z })
                    .unwrap();
            }
        }
    }
}
fn edit(id: u128, y: i32, block: BlockId) -> ClientMessage {
    ClientMessage::Edit {
        action_id: id,
        x: -1,
        y,
        z: -1,
        block,
        slot: 0,
    }
}
fn command(state: &mut State, message: ClientMessage, tick: u64) {
    crate::server::durable::coordinator::handle_live_message(state, 1, message, TickId::new(tick))
        .unwrap();
    crate::server::durable::coordinator::process_durable_actions(
        state,
        TickId::new(tick),
        Instant::now(),
    )
    .unwrap();
    crate::server::durable::complete_barrier(
        state,
        crate::server::durable::CommitBarrier::AllStaged,
    )
    .unwrap();
    assert!(state.durability.queued.is_empty());
}
fn tick_action(state: &mut State, id: EntityId, tick: u64) -> CommitAction {
    let mut results = crate::server::durable::entity_dispatch::plan_motion(
        state,
        TickId::new(tick),
        vec![DurableRequest::EntityTick { id }],
    );
    results.pop().unwrap().1.unwrap().unwrap()
}
fn public(state: &State, id: EntityId) -> Vec<u8> {
    let s = state.entities.snapshot(id).unwrap();
    state
        .entities
        .types()
        .descriptor(s.entity_type)
        .unwrap()
        .public_view(&s.private_payload)
        .unwrap()
}
fn settle_commit_action(state: &mut State, action: &CommitAction, tick: u64) {
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    assert!(
        state
            .durability
            .try_stage(TickId::new(tick), action, Some(permit))
            .unwrap()
    );
    crate::server::durable::complete_barrier(
        state,
        crate::server::durable::CommitBarrier::AllStaged,
    )
    .unwrap();
}

#[test]
fn fire_invalidates_two_cross_chunk_footprints_with_refunds_in_one_wal_record() {
    use bloxgloom_host_api::{Extension, Registrar, RegistrationError};
    struct Flammable;
    struct Gate<'a>(&'a mut dyn Registrar);
    impl Registrar for Gate<'_> {
        fn cube_block(
            &mut self,
            _: bloxgloom_host_api::CubeBlock,
        ) -> Result<(), RegistrationError> {
            Ok(())
        }
        fn inventory_screen(
            &mut self,
            _: bloxgloom_host_api::InventoryScreen,
        ) -> Result<(), RegistrationError> {
            unreachable!()
        }
        fn storage_block_entity(
            &mut self,
            _: bloxgloom_host_api::StorageBlockEntity,
        ) -> Result<(), RegistrationError> {
            unreachable!()
        }
        fn anchored_block_entity(
            &mut self,
            mut d: bloxgloom_host_api::anchored::AnchoredBlockEntity,
        ) -> Result<(), RegistrationError> {
            let catalog = crate::content::Catalog::builtins();
            let key = catalog.state(crate::world::WOOD).unwrap().key.to_string();
            d.block = "bloxgloom:wood".into();
            d.placement_item = "bloxgloom:wood".into();
            d.anchor_state = key.clone();
            for c in &mut d.footprint {
                c.state = key.clone();
            }
            self.0.anchored_block_entity(d)
        }
    }
    impl Extension for Flammable {
        fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
            bloxgloom_lifecycle_fixture::anchored::SignalPost.register(&mut Gate(r))
        }
    }
    let path = temp_save_dir("anchored-fire");
    let startup = || {
        ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&Flammable)
            .unwrap()
    };
    let mut state =
        crate::server::server_state_with_startup(7, path.clone(), 8, startup()).unwrap();
    for y in [79, 80] {
        state.world.get_chunk(world_to_chunk(2, y, 2).0).unwrap();
    }
    for x in [2, 4] {
        state.world.edit(x, 78, 2, crate::world::STONE).unwrap();
    }
    let item = ItemId(crate::world::WOOD.0);
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(item, 6));
    let peer = add_test_client(&mut state, [3.0, 79.0, 5.0], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    for (seq, x) in [(1, 2), (2, 4)] {
        command(
            &mut state,
            ClientMessage::Edit {
                action_id: epoch | seq,
                x,
                y: 79,
                z: 2,
                block: crate::world::WOOD,
                slot: 0,
            },
            10,
        );
        assert!(
            state
                .entities
                .anchored_at(CellCoord::new(x, 80, 2))
                .is_some()
        );
    }
    let (chunk, local) = world_to_chunk(3, 79, 2);
    let seed = state
        .fire
        .prepare_seed_from_edit(
            TickId::new(11),
            chunk,
            crate::world::Chunk::index(local).unwrap() as u16,
            crate::world::GLOWSTONE,
        )
        .unwrap()
        .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: state
            .world
            .prepare_edits(&[(3, 79, 2, crate::world::GLOWSTONE)])
            .unwrap(),
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: Some(seed.clone()),
        entity_wakes: vec![],
        entities: None,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(11), &action, None)
            .unwrap()
    );
    state.fire.mark_seed_submitted(&seed).unwrap();
    let barrier = |state: &mut State| {
        crate::server::durable::complete_barrier(
            state,
            crate::server::durable::CommitBarrier::AllStaged,
        )
        .unwrap()
    };
    barrier(&mut state);
    crate::server::durable::fire::run_delivery(&mut state, TickId::new(13)).unwrap();
    barrier(&mut state);
    state.durability.publish_queue.clear();
    crate::server::durable::fire::run_source(
        &mut state,
        TickId::new(11 + crate::server::fire::SPREAD_DELAY_TICKS),
    )
    .unwrap();
    assert!(state.durability.publish_queue.is_empty());
    assert_eq!(
        state.durability.pending.len(),
        1,
        "both invalidations share the source record"
    );
    for x in [2, 4] {
        assert!(
            state
                .entities
                .anchored_at(CellCoord::new(x, 80, 2))
                .is_some()
        );
    }
    barrier(&mut state);
    assert_eq!(state.durability.publish_queue.len(), 1);
    assert_eq!(
        state.durability.publish_queue[0].fire_bursts,
        vec![[2, 79, 2], [4, 79, 2]]
    );
    for x in [2, 4] {
        for y in [79, 80] {
            assert!(
                state
                    .entities
                    .anchored_at(CellCoord::new(x, y, 2))
                    .is_none()
            );
            assert_eq!(state.world.cached_block(x, y, 2), Some(crate::world::AIR));
        }
    }
    let count = |state: &State| {
        crate::server::drops::nearby(&state.entities, [3.0, 79.5, 2.5])
            .iter()
            .filter_map(|d| {
                crate::server::drops::stack(&state.entities, EntityId::new(d.id).unwrap())
            })
            .filter(|s| s.item == item)
            .map(|s| s.count)
            .sum::<u16>()
    };
    assert_eq!(count(&state), 4);
    drop(peer);
    drop(state);
    let mut state =
        crate::server::server_state_with_startup(7, path.clone(), 8, startup()).unwrap();
    assert_eq!(count(&state), 4);
    crate::server::durable::fire::run_source(
        &mut state,
        TickId::new(12 + crate::server::fire::SPREAD_DELAY_TICKS),
    )
    .unwrap();
    barrier(&mut state);
    assert_eq!(count(&state), 4);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn anchored_custom_state_cost_use_neighbor_support_and_recovery_are_atomic() {
    let path = temp_save_dir("anchored-behavior");
    let mut state = open(&path);
    resident(&mut state);
    state.world.edit(-1, 78, -1, crate::world::STONE).unwrap();
    state.world.edit(0, 79, -1, crate::world::AIR).unwrap();
    let catalog = state.world.catalog_arc();
    let block = catalog.state_by_key(KEY).unwrap();
    let item = catalog.items().find(|i| i.key == KEY).unwrap().id;
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(item, 2));
    let peer = add_test_client(&mut state, [-1.0, 79.0, 2.0], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    assert!(
        plan_durable_request(
            &mut state,
            &edit_request(edit(epoch | 1, 79, block)),
            TickId::new(9)
        )
        .is_err()
    );
    assert!(
        state
            .entities
            .anchored_at(CellCoord::new(-1, 79, -1))
            .is_none()
    );
    assert_eq!(
        state.clients[&1].inventory.slots[0],
        Some(Stack::new(item, 2))
    );
    // Refill the fixture before its successful placement. Rejected planning
    // above did not consume any inventory or reserve a footprint.
    state.clients.get_mut(&1).unwrap().inventory.slots[0] = Some(Stack::new(item, 6));
    let request = edit(epoch | 1, 79, block);
    let _placed = plan_durable_request(&mut state, &edit_request(request.clone()), TickId::new(10))
        .unwrap()
        .unwrap();
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        6
    );
    assert!(
        state
            .entities
            .anchored_at(CellCoord::new(-1, 79, -1))
            .is_none()
    );
    command(&mut state, request.clone(), 10);
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        3
    );
    command(&mut state, request, 11);
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        3
    );
    let id = state
        .entities
        .anchored_at(CellCoord::new(-1, 79, -1))
        .unwrap();
    let before = state.entities.snapshot(id).unwrap();
    let descriptor = state
        .entities
        .types()
        .descriptor(before.entity_type)
        .unwrap();
    let bytes = descriptor.encode_payload(&before.private_payload).unwrap();
    assert_eq!(bytes, vec![0, 0, 79, 0, 0, 0]);
    let mut payload = vec![4];
    payload.extend(id.get().to_le_bytes());
    payload.extend(before.revision.to_le_bytes());
    payload.extend(b"toggle");
    let use_request = ClientMessage::EntityInteract {
        action_id: epoch | 2,
        target: [-1, 80, -1],
        payload: payload.clone(),
    };
    command(&mut state, use_request.clone(), 12);
    command(&mut state, use_request, 13);
    assert!(
        plan_durable_request(
            &mut state,
            &edit_request(ClientMessage::EntityInteract {
                action_id: epoch | 3,
                target: [-1, 80, -1],
                payload
            }),
            TickId::new(14)
        )
        .is_err()
    );
    assert_eq!(public(&state, id), vec![1, 0]);
    // Capture on a worker, then invalidate an observed seam chunk. The entire
    // state/refund proposal must fail before admission, not publish stale state.
    let input = super::super::entity::capture_tick_input(&mut state, id, 30, false)
        .unwrap()
        .unwrap();
    let plan = input.plan().unwrap();
    state.world.edit(0, 79, -1, crate::world::STONE).unwrap();
    assert_eq!(
        super::super::entity::commit_tick_plan(&mut state, input, plan)
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    let action = tick_action(&mut state, id, 31);
    settle_commit_action(&mut state, &action, 31);
    assert_eq!(public(&state, id), vec![1, 1]);
    drop(peer);
    drop(state);
    let mut state = open(&path);
    resident(&mut state);
    assert_eq!(public(&state, id), vec![1, 1]);
    // A real durable player support edit is observed through the same worker
    // path after restart, even without relying on delivery of a wake hint.
    let inventory = state.inventory_store.load(17).unwrap();
    let peer = add_test_client(&mut state, [-1.0, 79.0, 2.0], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    command(&mut state, edit(epoch | 1, 78, crate::world::AIR), 60);
    let action = tick_action(&mut state, id, 61);
    assert!(state.entities.snapshot(id).is_some());
    assert_eq!(state.world.cached_block(-1, 80, -1), Some(block));
    settle_commit_action(&mut state, &action, 61);
    assert!(state.entities.snapshot(id).is_none());
    assert_eq!(
        state.world.cached_block(-1, 79, -1),
        Some(crate::world::AIR)
    );
    assert_eq!(
        state.world.cached_block(-1, 80, -1),
        Some(crate::world::AIR)
    );
    let refunds: u16 = crate::server::drops::nearby(&state.entities, [-0.5, 79.5, -0.5])
        .iter()
        .filter_map(|d| crate::server::drops::stack(&state.entities, EntityId::new(d.id).unwrap()))
        .filter(|s| s.item == item)
        .map(|s| s.count)
        .sum();
    assert_eq!(refunds, 2);
    drop(peer);
    drop(state);
    let state = open(&path);
    assert!(state.entities.snapshot(id).is_none());
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

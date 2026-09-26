use super::*;
use crate::server::durable::{CommitBarrier, complete_barrier};
use crate::server::entities::{EntityLocation, mossbun::Mossbun};

fn resident_platform(state: &mut State) {
    for x in -1..=1 {
        for y in 4..=6 {
            for z in -1..=1 {
                state
                    .world
                    .get_chunk(crate::world::ChunkKey { x, y, z })
                    .unwrap();
            }
        }
    }
    for x in -4..=5 {
        for z in -4..=5 {
            state.world.edit(x, 79, z, crate::world::STONE).unwrap();
        }
    }
}

fn live_command(state: &mut State, tick: u64, action_id: u128) {
    crate::server::durable::handle_live_message(
        state,
        1,
        ClientMessage::AdminSpawnMossbun { action_id },
        TickId::new(tick),
    )
    .unwrap();
    crate::server::durable::process_durable_actions(state, TickId::new(tick), Instant::now())
        .unwrap();
    complete_barrier(state, CommitBarrier::AllStaged).unwrap();
    assert!(state.durability.queued.is_empty());
    assert!(!state.durability.failed);
}

#[test]
fn mossbun_authorized_spawn_worker_steps_and_restart_preserve_identity() {
    let path = temp_save_dir("mossbun-live");
    let mut state = server_state(23, path.clone()).unwrap();
    resident_platform(&mut state);
    let _peer = add_test_client(&mut state, [0.5, 80.0, 0.5], Inventory::default());
    assert!(
        state
            .durability
            .request_epoch_grant(17, TickId::new(1))
            .unwrap()
            .is_none()
    );
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    let epoch = state
        .durability
        .request_epoch_grant(17, TickId::new(2))
        .unwrap()
        .unwrap();
    let action_id = |seq| (u128::from(epoch) << 64) | seq;
    let chunk = world_to_chunk(2, 80, 0).0;
    live_command(&mut state, 3, action_id(1));
    assert!(
        state
            .entities
            .public_views_for_chunk_bounded(chunk, 32)
            .unwrap()
            .is_empty()
    );
    state.admin_profile = Some(17);
    live_command(&mut state, 4, action_id(2));
    let views = state
        .entities
        .public_views_for_chunk_bounded(chunk, 32)
        .unwrap();
    assert_eq!(views.len(), 1);
    let id = views[0].id;
    assert_eq!(views[0].entity_type, crate::content::MOSSBUN_ENTITY_TYPE);
    assert_eq!(views[0].payload, [0, 0]);
    live_command(&mut state, 5, action_id(2)); // retry must not allocate a second identity
    assert_eq!(
        state
            .entities
            .public_views_for_chunk_bounded(chunk, 32)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(state.clients[&1].inventory, Inventory::default());
    state.clients.clear();
    // Run the production indexed due lane and worker dispatch with no clients.
    for tick in [8, 12] {
        crate::server::durable::queue_interaction_actions(&mut state, TickId::new(tick));
        crate::server::durable::process_durable_actions(
            &mut state,
            TickId::new(tick),
            Instant::now(),
        )
        .unwrap();
        complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    }
    let after = state.entities.snapshot(id).unwrap();
    assert_ne!(
        after.location,
        EntityLocation::Mobile {
            position: [2.5, 80.0, 0.5]
        }
    );
    assert_eq!(
        after
            .private_payload
            .downcast_ref::<Mossbun>()
            .unwrap()
            .cycle,
        1
    );
    assert_eq!(after.next_tick, Some(13));
    drop(state);
    let recovered = server_state(23, path.clone()).unwrap();
    let restored = recovered.entities.snapshot(id).unwrap();
    assert_eq!(restored.location, after.location);
    assert_eq!(restored.revision, after.revision);
    assert_eq!(restored.motion_revision, after.motion_revision);
    assert_eq!(restored.next_tick, after.next_tick);
    assert_eq!(
        restored.private_payload.downcast_ref::<Mossbun>(),
        after.private_payload.downcast_ref::<Mossbun>()
    );
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn missing_mossbun_terrain_defers_locally_then_runs_on_residency() {
    let path = temp_save_dir("mossbun-missing");
    let mut state = server_state(23, path.clone()).unwrap();
    resident_platform(&mut state);
    state.admin_profile = Some(17);
    let action = super::super::admin::plan_mossbun(&mut state, 17, [0.5, 80.0, 0.5], 1);
    // The live spawn/receipt/restart test above owns the integration setup;
    // this test checks capture's fail-closed retry before there is a worker.
    let batch = action.unwrap();
    let id = batch.entity_id();
    state.entities.apply_committed(batch).unwrap();
    state.world.reset_cache_for_test(128);
    let original = state.entities.snapshot(id).unwrap();
    assert_eq!(
        super::super::entity::capture_tick_input(&mut state, id, 5, false)
            .err()
            .unwrap()
            .kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(
        state.entities.snapshot(id).unwrap().next_tick,
        original.next_tick
    );
    resident_platform(&mut state);
    let input = super::super::entity::capture_tick_input(&mut state, id, 5, false)
        .unwrap()
        .unwrap();
    assert_eq!(input.neighbours.len(), 0);
    assert_eq!(input.view.revisions().len(), 27);
    assert!(input.plan().unwrap().payload.is_some());
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn mossbun_spawn_limit_rejects_without_allocating_or_consuming_items() {
    let path = temp_save_dir("mossbun-cap");
    let mut state = server_state(23, path.clone()).unwrap();
    resident_platform(&mut state);
    state.admin_profile = Some(17);
    for _ in 0..16 {
        let prepared =
            super::super::admin::plan_mossbun(&mut state, 17, [0.5, 80.0, 0.5], 1).unwrap();
        state.entities.apply_committed(prepared).unwrap();
    }
    assert_eq!(
        super::super::admin::plan_mossbun(&mut state, 17, [0.5, 80.0, 0.5], 1)
            .unwrap_err()
            .kind(),
        ErrorKind::QuotaExceeded
    );
    assert_eq!(
        state
            .entities
            .public_views_for_chunk_bounded(world_to_chunk(2, 80, 0).0, 32)
            .unwrap()
            .len(),
        16
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

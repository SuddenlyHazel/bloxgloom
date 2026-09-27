//! Contending player writes cannot starve simulation, or vice versa; distant
//! entity reads must still commit when one candidate in the wave conflicts.
use super::*;
use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
use crate::server::startup::StartupEntityType;
use std::sync::Arc;

#[test]
fn pickup_and_entity_backlogs_leave_command_room_and_one_actor_cannot_fill_it() {
    let path = temp_save_dir("response-command-capacity");
    let mut state = server_state(7, path.clone()).unwrap();
    for id in 1..=32 {
        state
            .durability
            .queued
            .push_back(DurableRequest::Pickup { id });
    }
    for raw in 1..=128 {
        state
            .durability
            .queued
            .push_back(DurableRequest::EntityTick {
                id: crate::server::entities::EntityId::new(raw).unwrap(),
            });
    }
    for sequence in 1..=9 {
        super::super::super::coordinator::handle_live_message(
            &mut state,
            1,
            ClientMessage::Edit {
                action_id: sequence,
                x: 2,
                y: 80,
                z: 3,
                block: crate::world::AIR,
                slot: 0,
            },
            TickId::new(1),
        )
        .unwrap();
    }
    super::super::super::coordinator::handle_live_message(
        &mut state,
        2,
        ClientMessage::Edit {
            action_id: 1,
            x: 2,
            y: 80,
            z: 3,
            block: crate::world::AIR,
            slot: 0,
        },
        TickId::new(1),
    )
    .unwrap();
    let actors = state
        .durability
        .queued
        .iter()
        .filter_map(|request| {
            if let DurableRequest::Command { id, .. } = request {
                Some(*id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(actors, [1, 1, 1, 1, 1, 1, 1, 1, 2]);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn continuous_edits_and_conflicting_entities_both_progress_without_blocking_distant_motion() {
    let path = temp_save_dir("response-fairness");
    let kind = crate::content::EntityTypeId(70_042);
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: kind,
            key: "test:fair_counter".into(),
            schema_version: 1,
            schema_fingerprint: 1,
        })
        .unwrap();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:fair_counter".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(CounterTick)),
    });
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    for x in [0, 2, 64] {
        state.world.get_chunk(world_to_chunk(x, 80, 3).0).unwrap();
    }
    state.world.edit(2, 80, 3, crate::world::AIR).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(
        crate::items::ItemId(crate::world::STONE.0),
        64,
    ));
    state.inventory_store.save(17, &inventory).unwrap();
    let _peer = add_test_client(&mut state, [0.5, 80.0, 0.5], inventory);
    let epoch = grant_action_epoch(&mut state, 17);
    let ids = stage_entity_spawn_batch(
        &mut state,
        [0.5, 64.5]
            .map(|x| EntitySpawn::Mobile {
                entity_type: kind,
                position: [x, 80.0, 0.5],
                payload: EntityPayload::new(7u8),
                spawn_tick: 1,
            })
            .to_vec(),
    );
    let before = ids
        .iter()
        .map(|id| state.entities.snapshot(*id).unwrap().revision)
        .collect::<Vec<_>>();
    for round in 1..=32u64 {
        let tick = TickId::new(10 + round * 5);
        super::super::super::coordinator::handle_live_message(
            &mut state,
            1,
            ClientMessage::Edit {
                action_id: (u128::from(epoch) << 64) | u128::from(round),
                x: 2,
                y: 80,
                z: 3,
                block: if round % 2 == 1 {
                    crate::world::STONE
                } else {
                    crate::world::AIR
                },
                slot: 0,
            },
            tick,
        )
        .unwrap();
        for id in &ids {
            state
                .durability
                .queued
                .push_back(DurableRequest::EntityTick { id: *id });
        }
        super::super::super::coordinator::process_durable_actions(&mut state, tick, Instant::now())
            .unwrap();
        assert!(state.durability.pending.is_empty());
        // Due records remain authoritative; avoid duplicate synthetic attempts.
        state
            .durability
            .queued
            .retain(|r| !matches!(r, DurableRequest::EntityTick { .. }));
    }
    assert!(
        state
            .durability
            .receipt_ledger(17)
            .results
            .iter()
            .filter(|r| r.accepted)
            .count()
            >= 20
    );
    assert!(
        state.entities.snapshot(ids[0]).unwrap().revision >= before[0] + 8,
        "a continuous player writer cannot starve the overlapping reader"
    );
    assert_eq!(
        state.entities.snapshot(ids[1]).unwrap().revision,
        before[1] + 32,
        "independent motion must not inherit a neighbour's admission conflict"
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

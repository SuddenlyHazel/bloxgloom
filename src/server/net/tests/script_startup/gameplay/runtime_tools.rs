//! Stable action streams and ordinary tooling through the authoritative planner.
use super::*;
use crate::server::gameplay::{OperationInput, Participants, plan_removals};
use bloxgloom_host_api::gameplay::Event;
#[test]
fn runtime_tools_action_retry_keeps_module_randomness_across_ticks_and_log_pressure() {
    let fixture = Fixture::new();
    fixture.action(
        REGISTER,
        r#"
        local count=math.random(1,128)
        return function(c,e)
            if string.byte(e.arguments,1)==1 then
                for i=1,90 do log.debug('attempt',{iteration=i,count=count}) end
            end
            assert(utf8.len('树')==1)
            local bytes=buffer.create(4); buffer.writeu32(bytes,0,count)
            assert(buffer.readu32(bytes,0)==count)
            assert(c.give('player',{item='bloxgloom:stick',count=count}))
        end
    "#,
    );
    let mut state = fixture.open().unwrap();
    let mut inventory = Inventory::default();
    inventory.slots.fill(None);
    let mut results = Vec::new();
    for (id, tick, logging) in [
        (99, 1, 0),
        (99, 500, 1),
        (100, 1, 0),
        (101, 1, 0),
        (102, 1, 0),
    ] {
        let plan = plan_removals(
            &mut state.world,
            &mut Default::default(),
            &mut Vec::new(),
            OperationInput {
                edits: &[],
                removals: &[],
                seed: 7,
                tick,
                action: Some(Event::ActionRequested {
                    action: "demo:shift".into(),
                    position: [0.5, 80., 0.5],
                    cell: None,
                    entity: None,
                    slot: 0,
                    arguments: vec![logging],
                }),
            },
            Participants {
                actor_inventory_revision: None,
                profile_inventories: None,
                profile_services: None,
                players: &[],
                action_id: Some(id),
                clock: None,
                weather: None,
                actor: Some((PROFILE, &inventory)),
                actor_position: Some([0.5, 80., 0.5]),
                admin: false,
                entities: &state.entities,
            },
        )
        .unwrap();
        results.push(plan.inventory.unwrap().slots[0].as_ref().unwrap().count);
    }
    assert_eq!(
        results[0], results[1],
        "retry tick or logging changed module initializer randomness"
    );
    assert!(
        results[2..].iter().any(|v| *v != results[0]),
        "distinct requests repeated the same stream"
    );
    assert!(
        inventory.slots.iter().all(Option::is_none),
        "planning published an item grant"
    );
}

//! One Luau invocation spanning every public gameplay transaction participant.
use super::*;

const DECLARE: &str = r#"return function(h)
    h.register_entity('demo:marker',1,1,1,nil)
    h.register_handler('demo:tick',1,'EntityTick','demo:marker','demo:action')
    h.register_action('demo:shift',1,'Shift','item','bloxgloom:stick','demo:action')
end"#;
const HANDLER: &str = r#"return function(c,e)
    if e.kind == 'EntityTick' then return end
    local before = c.inventory('player')[1].stack.count
    local taken = c.take('player',0,1)
    assert(taken.count == 1 and c.inventory('player')[1].stack.count == before-1)
    local block = c.block(2,80,0).state
    c.set_block(2,80,0,if block == 'bloxgloom:air' then 'bloxgloom:sand' else 'bloxgloom:glowstone')
    assert(c.block(2,80,0).state ~= block)
    c.spawn_stack(8.5,80.5,0.5,taken,4294967295)
    local nearby = c.nearby_entities(6.5,80.5,0.5,1)
    for _, entity in nearby do assert(entity.entity_type ~= 'bloxgloom:drop') end
    local marker
    for _, entity in nearby do
        if entity.entity_type == 'demo:marker' then marker = entity end
    end
    if marker then
        assert(c.update_entity(marker.id,string.char(2)))
        assert(c.entity_state(marker.id) == string.char(2))
        assert(c.schedule_entity(marker.id,100000))
    else
        c.spawn_entity('demo:marker',6.5,80.5,0.5,string.char(1))
    end
    if string.byte(e.arguments,1) == 2 then
        pcall(function() c.entity_state(c.tick) end)
    end
    if string.byte(e.arguments,1) == 1 then
        pcall(function() c.spawn_stack(8.5,80.5,0.5,{item='bloxgloom:stick',count=129},0) end)
    end
end"#;

#[test]
fn luau_world_entity_inventory_drop_and_schedule_are_one_retryable_receipt() {
    let fixture = Fixture::new();
    fixture.action(DECLARE, HANDLER);
    let mut state = Box::new(fixture.open().unwrap());
    state.spawn_anchor = [0.5, 80., 0.5];
    for (x, y, block) in [
        (0, 79, crate::world::STONE),
        (0, 80, AIR),
        (0, 81, AIR),
        (2, 80, AIR),
    ] {
        state.world.edit(x, y, 0, block).unwrap();
    }
    let catalog = state.world.catalog_arc();
    let stick = catalog.item_by_key("bloxgloom:stick").unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(stick, 3));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let first = peer.request(0);
        assert!(peer.send(&first).0);
        assert!(peer.send(&first).0, "duplicate receipt reapplied");
        peer.inventory_at(2);
        let forged = peer.request(2);
        assert!(
            !peer.send(&forged).0,
            "wrong-kind handle committed staged changes"
        );
        let failed = peer.request(1);
        assert!(!peer.send(&failed).0, "caught invalid stack committed");
    });
    let mut state = fixture.open().unwrap();
    let marker = state
        .entities
        .query_mobile_aabb([6., 80., 0.], [7., 81., 1.])
        .unwrap()
        .into_iter()
        .filter(|id| state.entities.public_view(*id).unwrap().payload == [1])
        .collect::<Vec<_>>();
    assert_eq!(marker.len(), 1);
    let marker = marker[0];
    assert_eq!(state.world.get_block(2, 80, 0).unwrap(), SAND);
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        2
    );
    assert_eq!(state.entities.public_view(marker).unwrap().payload, [1]);
    assert!(state.entities.snapshot(marker).unwrap().next_tick.is_none());
    drop(state);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        assert!(peer.send(&request).0);
        peer.inventory_at(1);
    });
    let mut state = fixture.open().unwrap();
    assert_eq!(state.world.get_block(2, 80, 0).unwrap(), GLOWSTONE);
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
    assert_eq!(state.entities.public_view(marker).unwrap().payload, [2]);
    assert!(state.entities.snapshot(marker).unwrap().next_tick.is_some());
    let drops = state
        .entities
        .record_values()
        .filter(|record| record.entity_type == crate::server::drops::DROP_ENTITY_TYPE)
        .count();
    assert_eq!(drops, 2);
}

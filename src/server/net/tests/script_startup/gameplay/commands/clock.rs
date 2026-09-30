//! Public clock control shares authority, rollback and WAL recovery with items/world.
use super::*;

#[test]
fn luau_clock_action_rolls_back_failures_and_recovers_atomic_edits_and_inventory() {
    let fixture = Fixture::new();
    fixture.action(
        "return function(h) h.register_action('demo:shift',1,'Clock','empty',nil,'demo:action') end",
        r#"return function(c,e)
            local time = c.world_time()
            assert(time.cycle_ms == 1200000)
            assert(not pcall(function() time.elapsed_ms = 0 end))
            assert(c.transfer(0,1,1))
            c.set_block(2,80,0,'bloxgloom:glowstone')
            c.admin_set_time(time.cycle_ms * 3 / 4)
            assert(c.world_time().elapsed_ms == time.cycle_ms * 3 / 4)
            local mode = string.byte(e.arguments,1)
            if mode == 1 then error('rollback clock') end
            if mode == 2 then pcall(function() c.admin_set_time(0.5) end) end
            if mode == 3 then pcall(function() c.admin_set_time(time.cycle_ms) end) end
            if mode == 4 then pcall(function() c.set_block(2.5,80,0,'bloxgloom:air') end) end
        end"#,
    );
    let mut original = None;
    let mut restore_inventory = None;
    for (admin, mode) in [
        (false, 0),
        (true, 1),
        (true, 2),
        (true, 3),
        (true, 4),
        (true, 0),
    ] {
        let mut state = Box::new(fixture.open().unwrap());
        state.admin_profile = admin.then_some(PROFILE);
        state.spawn_anchor = [0.5, 80.0, 0.5];
        state.world.edit(0, 79, 0, crate::world::STONE).unwrap();
        state.world.edit(0, 80, 0, AIR).unwrap();
        state.world.edit(0, 81, 0, AIR).unwrap();
        state.world.edit(2, 80, 0, AIR).unwrap();
        let mut inventory = Inventory::default();
        inventory.slots.fill(None);
        inventory.slots[0] = Some(Stack::new(crate::items::STICK, 3));
        state.inventory_store.save(PROFILE, &inventory).unwrap();
        restore_inventory = Some((state.inventory_store.clone(), inventory));
        let catalog = state.world.catalog_arc();
        let accepted = admin && mode == 0;
        serve(state, |address| {
            let mut peer = Peer::connect(address, catalog);
            peer.inventory_at(3);
            let request = peer.request(mode);
            let (result, reason) = peer.send(&request);
            assert_eq!(result, accepted, "admin={admin}, mode={mode}: {reason}");
            assert_eq!(
                peer.send(&request).0,
                accepted,
                "receipt replay changed outcome"
            );
            if accepted {
                peer.inventory_at(2);
                original = Some(request);
            }
        });
        // Every failed case is reopened before any successful write can mask it.
        let mut recovered = fixture.open().unwrap();
        assert_eq!(
            recovered.world.get_block(2, 80, 0).unwrap(),
            if accepted { GLOWSTONE } else { AIR }
        );
        let time = recovered.world_time.now();
        let expected = if accepted {
            crate::daylight::CYCLE_MS * 3 / 4
        } else {
            crate::daylight::INITIAL_MS
        };
        assert!(
            (expected..expected + 30_000).contains(&time),
            "clock partially changed: {time}"
        );
        assert_eq!(
            recovered.inventory_store.load(PROFILE).unwrap().slots[0]
                .as_ref()
                .unwrap()
                .count,
            if accepted { 2 } else { 3 }
        );
    }

    // Simulate a crash after the WAL receipt but before the clock/inventory
    // checkpoint: both recover from the same accepted action record.
    let path = fixture.0.join("save/world.time");
    let mut checkpoint = b"BGT2".to_vec();
    checkpoint.extend([0; 16]);
    checkpoint.extend(crate::daylight::INITIAL_MS.to_le_bytes());
    std::fs::write(path, checkpoint).unwrap();
    let (store, inventory) = restore_inventory.unwrap();
    store.save(PROFILE, &inventory).unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    assert!(
        (crate::daylight::CYCLE_MS * 3 / 4..crate::daylight::CYCLE_MS * 3 / 4 + 1000)
            .contains(&state.world_time.now())
    );
    let catalog = state.world.catalog_arc();
    state.admin_profile = Some(PROFILE);
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        peer.inventory_at(2);
        assert!(
            !peer.send(original.as_ref().unwrap()).0,
            "old session replay moved the clock"
        );
    });
}

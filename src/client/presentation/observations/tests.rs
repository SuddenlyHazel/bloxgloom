use super::*;
fn known() -> Observations {
    let mut slots = (0..36)
        .map(|slot| SlotView { slot, stack: None })
        .collect::<Vec<_>>();
    slots[0].stack = Some(StackView {
        item: "demo:stone".into(),
        count: 128,
        components: Some(ComponentView {
            version: 7,
            bytes: vec![0, 255, 1],
        }),
    });
    Observations {
        weather: None,
        pending_spawns: Default::default(),
        inventory: Some(InventoryView {
            revision: u64::MAX,
            slots,
        }),
        blocks: vec![BlockView {
            position: [-1, 80, 2],
            state: "demo:block[lit=true]".into(),
            version: u64::MAX,
        }],
        blocks_truncated: true,
        world: Some(WorldView {
            elapsed_ms: 300000,
            cycle_ms: crate::daylight::CYCLE_MS,
        }),
        actions: vec![ActionView {
            spawned: Default::default(),
            id: (1u128 << 64) | 2,
            key: Some("demo:use".into()),
            accepted: true,
            reason: String::new(),
        }],
    }
}
#[test]
fn observations_keep_exact_binary_revisions_and_nested_views_readonly() {
    let lua = Lua::new();
    lua.globals().set("r", known().lua(&lua).unwrap()).unwrap();
    lua.load(r#"
        assert(#r.inventory.slots==36 and r.inventory.slots[1].slot==0)
        assert(r.inventory.slots[2].slot==1 and r.inventory.slots[2].stack==nil)
        local stack=r.inventory.slots[1].stack
        assert(stack.item=='demo:stone' and stack.count==128)
        assert(stack.components.version==7 and stack.components.bytes==string.char(0,255,1))
        assert(type(r.inventory.revision)=='userdata')
        assert(r.inventory.revision_lo==4294967295 and r.inventory.revision_hi==4294967295)
        assert(r.blocks[1].revision==r.inventory.revision)
        assert(r.blocks[1].position.x==-1 and r.blocks[1].position.y==80 and r.blocks[1].position.z==2)
        assert(r.world.elapsed_ms==300000 and r.world.cycle_ms==1200000)
        assert(r.actions[1].id=='00000000000000010000000000000002')
        assert(r.blocks_truncated==true and r.unknown==nil)
        for _,t in ipairs({r,r.inventory,r.inventory.slots,r.inventory.slots[1],stack,stack.components,r.blocks,r.blocks[1],r.blocks[1].position,r.world,r.actions,r.actions[1]}) do
            assert(not pcall(function()t.changed=true end))
        end
    "#).exec().unwrap();
}
#[test]
fn observations_distinguish_unknown_from_known_empty_and_filter_action_ownership() {
    let lua = Lua::new();
    let empty = Observations::default();
    lua.globals()
        .set("unknown", empty.lua(&lua).unwrap())
        .unwrap();
    let mut known = known();
    for slot in &mut known.inventory.as_mut().unwrap().slots {
        slot.stack = None;
    }
    known.actions.push(ActionView {
        spawned: Default::default(),
        id: (1u128 << 64) | 3,
        key: None,
        accepted: false,
        reason: "Denied".into(),
    });
    known.actions.push(ActionView {
        spawned: Default::default(),
        id: (1u128 << 64) | 4,
        key: Some("other:use".into()),
        accepted: true,
        reason: String::new(),
    });
    lua.globals()
        .set("known", known.for_owner("demo").lua(&lua).unwrap())
        .unwrap();
    lua.load("assert(unknown.inventory==nil and unknown.world==nil); assert(#unknown.blocks==0 and #unknown.actions==0); assert(#known.inventory.slots==36); for _,slot in ipairs(known.inventory.slots) do assert(slot.stack==nil) end; assert(#known.actions==1 and known.actions[1].key=='demo:use')").exec().unwrap();
}
#[test]
fn observations_reject_invalid_dense_inventory_components_world_and_window_bounds() {
    let mut bad = known();
    bad.inventory.as_mut().unwrap().slots.pop();
    assert!(bad.validate().is_err());
    let mut bad = known();
    bad.inventory.as_mut().unwrap().slots[2].slot = 1;
    assert!(bad.validate().is_err());
    for count in [0, 129] {
        let mut bad = known();
        bad.inventory.as_mut().unwrap().slots[0]
            .stack
            .as_mut()
            .unwrap()
            .count = count;
        assert!(bad.validate().is_err());
    }
    let mut bad = known();
    bad.inventory.as_mut().unwrap().slots[0]
        .stack
        .as_mut()
        .unwrap()
        .components
        .as_mut()
        .unwrap()
        .bytes = vec![1; 1025];
    assert!(bad.validate().is_err());
    let mut bad = known();
    bad.world.as_mut().unwrap().elapsed_ms = crate::daylight::CYCLE_MS;
    assert!(bad.validate().is_err());
    let mut bad = known();
    bad.blocks = (0..65)
        .map(|x| BlockView {
            position: [x, 0, 0],
            state: "demo:block".into(),
            version: 0,
        })
        .collect();
    assert!(bad.validate().is_err());
    let mut bad = known();
    bad.actions[0].reason = "x".repeat(33);
    bad.actions[0].accepted = false;
    assert!(bad.validate().is_err());
}

#[test]
fn committed_spawn_ordinals_expose_exact_readonly_entity_handles() {
    let lua = Lua::new();
    let mut observations = known();
    observations.actions[0].accepted = true;
    observations.actions[0].reason.clear();
    observations.actions[0].spawned = vec![crate::protocol::SpawnReceipt {
        ordinal: 0,
        entity: 9_007_199_254_740_993,
    }];
    lua.globals()
        .set("replica", observations.lua(&lua).unwrap())
        .unwrap();
    lua.load("local launch=replica.actions[1].spawned[1]; assert(launch.ordinal==0); assert(tostring(launch.entity)=='entity:0020000000000001'); assert(not pcall(function() launch.ordinal=1 end)); assert(not pcall(function() replica.actions[1].spawned[1]=nil end))").exec().unwrap();
}

#[test]
fn weather_observations_are_optional_readonly_and_reject_nonfinite_values() {
    let lua = Lua::new();
    lua.globals()
        .set("r", Observations::default().lua(&lua).unwrap())
        .unwrap();
    lua.load("assert(r.weather==nil)").exec().unwrap();
    let mut observations = Observations::default();
    let mut snapshot = crate::weather::WeatherSnapshot::initial(7);
    snapshot.to = crate::weather::WeatherKind::Rain;
    observations.weather = Some(snapshot.observation(0));
    lua.globals()
        .set("r", observations.lua(&lua).unwrap())
        .unwrap();
    lua.load("assert(r.weather.kind=='rain' and r.weather.rain_mm_h==18); assert(not pcall(function() r.weather.kind='clear' end)); assert(r.weather.admin_set_weather==nil)").exec().unwrap();
    observations.weather.as_mut().unwrap().rain_mm_h = f32::NAN;
    assert!(observations.lua(&lua).is_err());
}

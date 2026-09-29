use super::*;

#[test]
fn mod_admin_grant_requires_server_identity_and_replays_once_over_listener() {
    let fixture = Fixture::new();
    fixture.action(
        &REGISTER.replace("'item', 'bloxgloom:stick'", "'empty', nil"),
        "return function(c,e) assert(c.admin_give('bloxgloom:stick',1)) end",
    );
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        let (accepted, reason) = peer.send(&request);
        assert!(!accepted, "{reason}");
        assert!(reason.contains("admin access denied"), "{reason}");
        assert_eq!(peer.inventory.slots[0], None);
    });
    let mut state = Box::new(fixture.open().unwrap());
    state.admin_profile = Some(PROFILE);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        assert!(peer.send(&request).0);
        assert!(peer.send(&request).0, "receipt retry must not grant again");
        peer.inventory_at(1);
    });
    let mut state = Box::new(fixture.open().unwrap());
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0],
        Some(Stack::new(crate::items::STICK, 1))
    );
    state.admin_profile = Some(PROFILE);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        peer.inventory_at(1);
        let request = peer.request(0);
        assert!(peer.send(&request).0);
        peer.inventory_at(2);
    });
}

#[test]
fn mod_admin_spawn_and_drop_share_one_allocator_and_restart() {
    let fixture = Fixture::new();
    fixture.action(
        &REGISTER.replace("'item', 'bloxgloom:stick'", "'empty', nil"),
        "return function(c,e) c.spawn_drop(4,80,0,'bloxgloom:stick',1,1500); if string.byte(e.arguments,1) == 1 then c.set_block(2,80,0,'bloxgloom:stone') end; c.admin_spawn('bloxgloom:mossbun') end",
    );
    let mut state = Box::new(fixture.open().unwrap());
    state.admin_profile = Some(PROFILE);
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -5..=5 {
        for z in -5..=5 {
            for y in 79..=83 {
                state
                    .world
                    .edit(x, y, z, if y == 79 { crate::world::STONE } else { AIR })
                    .unwrap();
            }
        }
    }
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let conflict = peer.request(1);
        assert!(
            !peer.send(&conflict).0,
            "spawn must not validate pre-edit ground"
        );
        let request = peer.request(0);
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        assert!(peer.send(&request).0);
    });
    let mut state = fixture.open().unwrap();
    assert_eq!(state.world.get_block(2, 80, 0).unwrap(), AIR);
    let views = state
        .entities
        .public_views_for_chunk_bounded(crate::world::world_to_chunk(2, 80, 0).0, 32)
        .unwrap();
    assert_eq!(
        views
            .iter()
            .filter(|view| view.entity_type == crate::content::MOSSBUN_ENTITY_TYPE)
            .count(),
        1
    );
    assert_eq!(
        views
            .iter()
            .filter(|view| view.entity_type == crate::server::drops::DROP_ENTITY_TYPE)
            .count(),
        1
    );
    assert_ne!(views[0].id, views[1].id);
}

#[test]
fn luau_creature_replaces_itself_with_another_authored_type_over_real_listener() {
    let fixture = Fixture::new();
    let parent = "h.register_creature{key='demo:parent',module='demo:creature',schema=1,revision=1,max_state_bytes=16,initial_state='parent',interval=1,read_radius=1,body={half_width=0.25,height=0.7,speed=1.0},model={{min={-0.2,0,-0.2},max={0.2,0.7,0.2},color={0.2,0.8,0.3}}}}";
    let child = "h.register_creature{key='demo:child',module='demo:creature',schema=1,revision=1,max_state_bytes=16,initial_state='child',interval=1,read_radius=1,body={half_width=0.25,height=0.7,speed=1.0},model={{min={-0.2,0,-0.2},max={0.2,0.7,0.2},color={0.2,0.8,0.3}}}}";
    let register = REGISTER
        .replace("'item', 'bloxgloom:stick'", "'empty', nil")
        .replace(" end", &format!("; {parent}; {child} end"));
    fixture.action(
        &register,
        "return function(c,e) c.admin_spawn('demo:parent') end",
    );
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}requires bloxgloom:content/v1\nrequires bloxgloom:mobile_entities/v1\nmodule creature creature.luau\n"),
    )
    .unwrap();
    std::fs::write(
        fixture.0.join("packages/demo/creature.luau"),
        "return function(c) if c.data == 'parent' then return 'done',10,nil,nil,{despawn=true,spawns={{key='demo:child',position={c.position[1]+1,c.position[2],c.position[3]}}}} end return c.data,10,nil,nil end",
    ).unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    state.admin_profile = Some(PROFILE);
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -8..=8 {
        for z in -8..=8 {
            for y in 79..=83 {
                state
                    .world
                    .edit(x, y, z, if y == 79 { crate::world::STONE } else { AIR })
                    .unwrap();
            }
        }
    }
    let catalog = state.world.catalog_arc();
    let parent_id = catalog.entity_type_id_by_key("demo:parent").unwrap();
    let child_id = catalog.entity_type_id_by_key("demo:child").unwrap();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        assert!(
            peer.send(&request).0,
            "retry must not spawn a second parent"
        );
        std::thread::sleep(std::time::Duration::from_millis(400));
    });
    let state = fixture.open().unwrap();
    let entities = state
        .entities
        .query_mobile_aabb([-8.0, 79.0, -8.0], [8.0, 83.0, 8.0])
        .unwrap();
    let mut parents = 0;
    let mut children = 0;
    for id in entities {
        let snapshot = state.entities.snapshot(id).unwrap();
        if snapshot.entity_type == parent_id {
            parents += 1;
        }
        if snapshot.entity_type == child_id {
            children += 1;
            let definition = state.world.catalog().mobile_entity(child_id).unwrap();
            let bytes = definition
                .behavior
                .encode(&snapshot.private_payload)
                .unwrap();
            assert_eq!(&bytes[12..], b"child");
        }
    }
    assert_eq!((parents, children), (0, 1));
}

#[test]
fn luau_action_block_targets_keep_real_reach_sight_and_identity_checks() {
    let fixture = Fixture::new();
    fixture.action(
        &REGISTER.replace("'item', 'bloxgloom:stick'", "'block', 'bloxgloom:stone'"),
        r#"return function(c,e)
            assert(e.cell[1] == 2 and e.cell[2] == 80 and e.cell[3] == 0)
            assert(c.block(2,80,0).block_type == 'bloxgloom:stone')
            c.set_block(2,80,0,'bloxgloom:glowstone')
        end"#,
    );
    let mut state = Box::new(fixture.open().unwrap());
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for (x, y, z, block) in [
        (0, 79, 0, crate::world::STONE),
        (0, 80, 0, AIR),
        (0, 81, 0, AIR),
        (1, 80, 0, AIR),
        (1, 81, 0, AIR),
        (2, 81, 0, AIR),
        (2, 80, 0, crate::world::STONE),
        (2, 80, 2, AIR),
        (3, 80, 0, crate::world::STONE),
        (3, 81, 0, crate::world::STONE),
        (4, 80, 0, crate::world::STONE),
        (10, 80, 0, crate::world::STONE),
        (64, 80, 0, crate::world::STONE),
    ] {
        state.world.edit(x, y, z, block).unwrap();
    }
    let terrain_version = state
        .world
        .cached_version(crate::world::world_to_chunk(2, 80, 0).0)
        .unwrap();
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        for (target, expected_reason) in [
            ([64, 80, 0], "interest"),
            ([10, 80, 0], "reach"),
            ([2, 80, 2], "changed"),
            ([4, 80, 0], "occluded"),
        ] {
            let mut request = peer.request(0);
            if let ClientMessage::EntityInteract {
                target: at,
                payload,
                ..
            } = &mut request
            {
                *at = target;
                *payload = bloxgloom_host_api::actions::TerrainRequest {
                    version: terrain_version,
                    request: Request::decode(payload).unwrap(),
                }
                .encode()
                .unwrap();
            }
            let (accepted, reason) = peer.send(&request);
            assert!(!accepted, "{reason}");
            assert!(reason.contains(expected_reason), "{reason}");
        }
        let mut request = peer.request(0);
        if let ClientMessage::EntityInteract {
            target, payload, ..
        } = &mut request
        {
            *target = [2, 80, 0];
            *payload = bloxgloom_host_api::actions::TerrainRequest {
                version: terrain_version,
                request: Request::decode(payload).unwrap(),
            }
            .encode()
            .unwrap();
        }
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        // A fresh action cannot re-use a target whose authoritative type changed.
        let mut request = peer.request(0);
        if let ClientMessage::EntityInteract {
            target, payload, ..
        } = &mut request
        {
            *target = [2, 80, 0];
            *payload = bloxgloom_host_api::actions::TerrainRequest {
                version: terrain_version,
                request: Request::decode(payload).unwrap(),
            }
            .encode()
            .unwrap();
        }
        assert!(!peer.send(&request).0);
    });
    let mut recovered = fixture.open().unwrap();
    assert_eq!(recovered.world.get_block(2, 80, 0).unwrap(), GLOWSTONE);
    assert_eq!(
        recovered.world.get_block(4, 80, 0).unwrap(),
        crate::world::STONE
    );
    assert_eq!(
        recovered.world.get_block(64, 80, 0).unwrap(),
        crate::world::STONE
    );
}

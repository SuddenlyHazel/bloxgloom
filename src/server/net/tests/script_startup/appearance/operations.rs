//! Script cosmetic replacement uses the native palette/save/replication contract.
use super::*;

fn fixture(authority: bool) -> Fixture {
    let fixture = Fixture::new();
    let mut requires =
        format!("{CONTENT}\nrequires bloxgloom:actions/v1\nmodule action action.luau");
    if authority {
        requires.push_str("\nrequires bloxgloom:players/v1");
    }
    fixture.package(
        "demo",
        &requires,
        &source("h.register_action('demo:shift',1,'Change appearance','empty',nil,'demo:action')"),
    );
    std::fs::write(fixture.0.join("packages/demo/action.luau"),r#"
        return function(c,e)
            local target=nil
            for _,p in c.players() do if p.profile~=c.player_profile then target=p end end
            assert(target)
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            local mode=string.byte(e.arguments,1)
            c.set_player_appearance(target.session,6,8,6)
            local projected=c.player_by_session(target.session)
            assert(projected.appearance.skin==6 and projected.appearance.shirt==8 and projected.appearance.pants==6)
            assert(not pcall(function() projected.appearance.skin=0 end))
            if mode==1 then error('reject cosmetic') end
            if mode==2 then pcall(function() c.set_player_appearance(target.session,255,8,6) end) end
            if mode==3 then pcall(function() c.set_player_appearance(target.session,6.5,8,6) end) end
        end
    "#).unwrap();
    fixture
}

impl Peer {
    fn request(&mut self, mode: u8) -> ClientMessage {
        let action_id = (u128::from(self.epoch) << 64) | u128::from(self.next_seq);
        self.next_seq += 1;
        ClientMessage::EntityInteract {
            action_id,
            target: [0; 3],
            payload: bloxgloom_host_api::actions::Request {
                key: "demo:shift".into(),
                version: 1,
                slot: 0,
                inventory_revision: self.inventory_revision,
                entity: 0,
                entity_revision: 0,
                arguments: vec![mode],
            }
            .encode()
            .unwrap(),
        }
    }
    fn action(&mut self, request: &ClientMessage) -> bool {
        let ClientMessage::EntityInteract { action_id, .. } = request else {
            unreachable!()
        };
        protocol::write_client(&mut self.stream, request).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let ServerMessage::ActionResult {
                action_id: id,
                accepted,
                ..
            } = self.read(deadline)
                && id == *action_id
            {
                return accepted;
            }
        }
    }
}

#[test]
fn luau_player_appearance_is_authorized_rollback_safe_peer_replicated_and_saved() {
    for authority in [false, true] {
        let fixture = fixture(authority);
        for restarted in [false, true] {
            let state = Box::new(fixture.open().unwrap());
            let catalog = state.world.catalog_arc();
            gameplay::serve(state, |address| {
                let mut actor = Peer::connect(address, 0xa991, &catalog);
                let mut target = Peer::connect(address, 0xa992, &catalog);
                let a = actor.own;
                let b = target.own;
                let initial = if authority && restarted {
                    [6, 8, 6, 0]
                } else {
                    [0; 4]
                };
                actor.appearance(a, [0; 4]);
                actor.appearance(b, initial);
                target.appearance(b, initial);
                if !restarted {
                    for mode in [1, 2, 3] {
                        let request = actor.request(mode);
                        assert!(!actor.action(&request));
                    }
                    actor.barrier();
                    assert_eq!(actor.views[&b].payload, [0; 4]);
                    assert_eq!(actor.views[&b].revision, 1);
                    let request = actor.request(0);
                    assert_eq!(actor.action(&request), authority);
                    if authority {
                        assert!(actor.action(&request), "duplicate receipt rejected");
                        assert_eq!(actor.appearance(b, [6, 8, 6, 0]), 2);
                        assert_eq!(target.appearance(b, [6, 8, 6, 0]), 2);
                    }
                    actor.barrier();
                    assert_eq!(
                        actor.views[&a].payload, [0; 4],
                        "changed the caller instead of target"
                    );
                }
            });
            let state = fixture.open().unwrap();
            assert_eq!(
                state
                    .appearance_store
                    .load(0xa992, state.world.catalog())
                    .unwrap()
                    .legacy(),
                if authority { [6, 8, 6, 0] } else { [0; 4] }
            );
            assert_eq!(
                state.inventory_store.load(0xa991).unwrap().slots[0]
                    .as_ref()
                    .map(|s| s.count),
                authority.then_some(1)
            );
            assert!(
                state
                    .inventory_store
                    .load(0xa992)
                    .unwrap()
                    .slots
                    .iter()
                    .all(Option::is_none)
            );
        }
    }
}

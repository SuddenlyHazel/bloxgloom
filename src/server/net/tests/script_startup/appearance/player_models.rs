//! Package rig and profile appearance exercise the production listener and
//! verified bundle path, including peer replication and process restart.
use super::*;
use crate::appearance::{AppearanceState, PackagedAppearance};
use bloxgloom_host_api::entity::{Tint, TintMode, VisualState};
fn fixture() -> Fixture {
    let f = Fixture::new();
    let dir = f.0.join("packages/demo");
    std::fs::create_dir_all(dir.join("assets/models")).unwrap();
    std::fs::create_dir_all(dir.join("server")).unwrap();
    std::fs::write(dir.join("package.txt"),"format 2\npackage demo\nversion 1.0.0\nentry main\nrequires bloxgloom:content/v1\nrequires bloxgloom:players/v1\nrequires bloxgloom:actions/v1\nmodule server main server/main.luau\nmodule server action server/action.luau\nasset model player assets/models/model.glb\nasset model-controls looks assets/models/controls.json\n").unwrap();
    std::fs::write(
        dir.join("assets/models/model.glb"),
        include_bytes!("../../../../../../fixtures/authored-model/model.glb"),
    )
    .unwrap();
    std::fs::write(
        dir.join("assets/models/controls.json"),
        include_bytes!("../../../../../../fixtures/authored-model/controls.json"),
    )
    .unwrap();
    std::fs::write(dir.join("server/main.luau"),source("h.register_player_model{key='demo:player',asset='demo:player',controls='demo:looks',clips={idle='idle',walk='bounce',run='bounce',crouch='nod'}};h.register_model{key='demo:ordinary',asset='demo:player'};h.register_action('demo:model',1,'Model test','empty',nil,'demo:action')")).unwrap();
    std::fs::write(dir.join("server/action.luau"),r#"return function(c,e)
        local me=c.player_by_profile(c.player_profile);assert(me and me.model=='demo:player')
        local mode=string.byte(e.arguments,1)
        if mode==0 then c.play_player_animation(me.session,{clip='nod',looping=true,crossfade_ms=100})
        elseif mode==1 then c.give('player',{item='bloxgloom:stick',count=1});pcall(function() c.set_player_model_layer(me.session,'missing',true) end)
        elseif mode==2 then c.stop_player_animation(me.session,150)
        elseif mode==3 then c.set_player_model(me.session,'demo:ordinary')
        elseif mode==4 then c.set_player_model_layer(me.session,'hat',true);c.set_player_model_variant(me.session,'eyes','sleepy');c.set_player_model_tint(me.session,'iris',{rgb={50,150,250},mode='replace'})
        end
    end"#).unwrap();
    f
}
impl Peer {
    fn model_request(&mut self, mode: u8) -> ClientMessage {
        let action_id = (u128::from(self.epoch) << 64) | u128::from(self.next_seq);
        self.next_seq += 1;
        ClientMessage::EntityInteract {
            action_id,
            target: [0; 3],
            payload: bloxgloom_host_api::actions::Request {
                key: "demo:model".into(),
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
    fn model_action(&mut self, mode: u8) -> bool {
        let message = self.model_request(mode);
        let ClientMessage::EntityInteract { action_id, .. } = message else {
            unreachable!()
        };
        self.send(message);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let ServerMessage::ActionResult {
                action_id: id,
                accepted,
                ..
            } = self.read(deadline)
                && id == action_id
            {
                return accepted;
            }
        }
    }
    fn model_state(
        &mut self,
        id: u64,
        predicate: impl Fn(AppearanceState) -> bool,
    ) -> AppearanceState {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(appearance) = self
                .views
                .get(&id)
                .and_then(|v| AppearanceState::decode(&v.payload))
                && predicate(appearance)
            {
                return appearance;
            }
            self.read(deadline);
        }
    }
}
#[test]
fn packaged_player_profiles_looks_and_script_clips_replicate_and_restart_without_playback() {
    let fixture = fixture();
    let mut saved_look = None;
    for restarted in [false, true] {
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        let model = catalog.player_model_id("demo:player").unwrap();
        gameplay::serve(state, |address| {
            let mut first = Peer::connect(address, 0xa991, &catalog);
            let own = first.own;
            if !restarted {
                first.model_state(own, |s| s.packaged.is_none());
                let mut visual = VisualState::default();
                visual.layers[0] = 0;
                first.send(ClientMessage::SelectPlayerModel {
                    packaged: Some(PackagedAppearance { model, visual }),
                });
                first.model_state(own, |s| {
                    s.packaged
                        .is_some_and(|p| p.model == model && p.visual.layers[0] == 0)
                });
                assert!(first.model_action(4));
                let appearance = first.model_state(own, |s| {
                    s.packaged
                        .is_some_and(|p| p.visual.variants[0] == 1 && p.visual.layers[1] == 1)
                });
                assert_eq!(
                    appearance.packaged.unwrap().visual.tints[1],
                    Some(Tint {
                        rgb: [50, 150, 250],
                        mode: TintMode::Replace
                    })
                );
                saved_look = Some(appearance);
                assert!(first.model_action(0));
                first.model_state(own, |s| {
                    s.packaged
                        .is_some_and(|p| p.visual.playback.is_some_and(|p| p.looping))
                });
            } else {
                let appearance = first.model_state(own, |s| s.packaged.is_some());
                assert_eq!(
                    appearance,
                    AppearanceState {
                        packaged: saved_look
                            .unwrap()
                            .packaged
                            .map(PackagedAppearance::durable),
                        ..saved_look.unwrap()
                    },
                    "restart retains selected rig and look while discarding transient playback"
                );
                saved_look = Some(appearance);
            }
            let mut second = Peer::connect(address, 0xa992, &catalog);
            let peer_state = second.model_state(own, |s| s.packaged.is_some());
            assert_eq!(peer_state.packaged.unwrap().model, model);
            if !restarted {
                assert!(peer_state.packaged.unwrap().visual.playback.is_some());
                assert!(first.model_action(2));
                saved_look = Some(first.model_state(own, |s| {
                    s.packaged.is_some_and(|p| p.visual.playback.is_none())
                }));
                second.model_state(own, |s| {
                    s.packaged.is_some_and(|p| p.visual.playback.is_none())
                });
                let path = fixture
                    .0
                    .join("save/players/0000000000000000000000000000a991.appearance");
                let bytes = std::fs::read(&path).unwrap();
                let inventory = first.inventory_revision;
                assert!(!first.model_action(1));
                assert!(!first.model_action(3));
                first.barrier();
                assert_eq!(first.inventory_revision, inventory);
                assert_eq!(std::fs::read(&path).unwrap(), bytes);
                assert_eq!(
                    first.model_state(own, |s| s.packaged.is_some()),
                    saved_look.unwrap()
                );
                assert!(first.model_action(0));
                first.model_state(own, |s| {
                    s.packaged.is_some_and(|p| p.visual.playback.is_some())
                });
            } else {
                first.send(ClientMessage::SelectAppearance {
                    palettes: [6, 8, 6],
                });
                let state = first.model_state(own, |s| s.palettes == [6, 8, 6]);
                assert_eq!(state.packaged, saved_look.unwrap().packaged);
                first.send(ClientMessage::SelectCharacter { recipe: None });
                first.model_state(own, |s| s.packaged.is_none() && s.palettes == [6, 8, 6]);
                second.model_state(own, |s| s.packaged.is_none());
            }
        });
    }
}

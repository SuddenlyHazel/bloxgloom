//! Downloaded dynamic controls/lists -> production UI action -> durable economy.
use super::*;
use crate::client::PackageActionProbe;
use crate::inventory::Stack;
use crate::ui::authored::{EguiIntent, Session};

const PROFILE: u128 = 0xCE91;
const AIM: [i32; 3] = [2, 81, 0];

fn open(fixture: &Fixture) -> State {
    let packages =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe-browser/packages");
    crate::server::server_state_with_startup(
        7,
        fixture.0.join("save"),
        2,
        ServerStartup::new(Arc::new(Catalog::builtins()))
            .with_local_packages(&packages)
            .unwrap(),
    )
    .unwrap()
}

fn change(session: &mut Session, id: &str, value: &str) {
    let index = session.node_index(id).expect("declared control exists");
    session.apply_egui(EguiIntent::Input(index, value.into()));
    session.wait_for_presentation().unwrap();
}

fn activate(session: &mut Session, id: &str) {
    let index = session.node_index(id).expect("dynamic button exists");
    session.apply_egui(EguiIntent::Activate(index));
    session.wait_for_presentation().unwrap();
}

#[test]
fn recipe_browser_dynamic_controls_real_server_crafting_rollback_replay_and_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let mut expected_revision = 0;
    for restarted in [false, true] {
        let mut state = Box::new(open(&fixture));
        state.spawn_anchor = [0.5, 80.0, 0.5];
        let catalog = state.world.catalog_arc();
        let stone = catalog.item_by_key("bloxgloom:stone").unwrap();
        let gravel = catalog.item_by_key("bloxgloom:gravel").unwrap();
        if !restarted {
            for x in -1..=4 {
                for z in -1..=2 {
                    for y in 79..=83 {
                        state
                            .world
                            .edit(
                                x,
                                y,
                                z,
                                if y == 79 {
                                    crate::world::STONE
                                } else {
                                    crate::world::AIR
                                },
                            )
                            .unwrap();
                    }
                }
            }
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::new(stone, 6));
            inventory.slots[1] = Some(Stack::new(gravel, 127));
            inventory.slots[2] = Stack::with_components(stone, 4, 1, vec![0, 255]);
            for slot in &mut inventory.slots[3..] {
                *slot = Some(Stack::new(crate::items::STICK, 128));
            }
            state.inventory_store.save(PROFILE, &inventory).unwrap();
        }
        gameplay::serve(state, |address| {
            let mut peer = PackageActionProbe::connect_document(
                &address.to_string(),
                PROFILE,
                fixture.0.join("recipe-config"),
            );
            peer.ready(AIM, crate::world::AIR, expected_revision);
            assert!(
                peer.session_mut()
                    .node_index("recipe:browser/choose_stone")
                    .is_none(),
                "fresh sessions start with static document"
            );
            peer.open_recipe_binding();
            assert!(
                peer.session_mut()
                    .node_index("recipe:browser/choose_stone")
                    .is_some()
            );
            assert!(
                peer.session_mut()
                    .node_index("recipe:browser/choose_gravel")
                    .is_some()
            );
            change(peer.session_mut(), "recipe:browser/search", "press");
            assert!(
                peer.session_mut()
                    .node_index("recipe:browser/choose_stone")
                    .is_none()
            );
            assert!(
                peer.session_mut()
                    .node_index("recipe:browser/choose_gravel")
                    .is_some()
            );
            change(peer.session_mut(), "recipe:browser/search", "");
            change(peer.session_mut(), "recipe:browser/category", "crushing");
            assert!(
                peer.session_mut()
                    .node_index("recipe:browser/choose_gravel")
                    .is_none()
            );
            change(peer.session_mut(), "recipe:browser/category", "all");
            change(peer.session_mut(), "recipe:browser/selected_only", "true");
            assert!(
                peer.session_mut()
                    .node_index("recipe:browser/choose_stone")
                    .is_some()
            );
            assert!(
                peer.session_mut()
                    .node_index("recipe:browser/choose_gravel")
                    .is_none()
            );
            change(peer.session_mut(), "recipe:browser/selected_only", "false");
            change(
                peer.session_mut(),
                "recipe:browser/notes",
                "Local recipe notes\nKeep exact input components",
            );
            if !restarted {
                activate(peer.session_mut(), "recipe:browser/craft");
                let request = peer.submit_ui_action();
                // Retry while the receipt is outstanding. The actual client
                // acknowledges results, after which old IDs are intentionally
                // rejected instead of retaining their receipt indefinitely.
                peer.resend(&request);
                let result = peer.result(&request);
                assert!(result.0, "initial craft rejected: {}", result.1);
                assert!(peer.result(&request).0, "replay returns same receipt");
                assert_eq!(peer.inventory_count(0), 5);
                assert_eq!(peer.inventory_count(1), 128);
                change(peer.session_mut(), "recipe:browser/quantity", "2");
                activate(peer.session_mut(), "recipe:browser/craft");
                let denied = peer.submit_ui_action();
                assert!(
                    !peer.result(&denied).0,
                    "full output must reject the whole take+give"
                );
                assert_eq!(peer.inventory_count(0), 5);
                assert_eq!(peer.inventory_count(1), 128);
                peer.select_slot(2);
                activate(peer.session_mut(), "recipe:browser/craft");
                let components = peer.submit_ui_action();
                assert!(
                    !peer.result(&components).0,
                    "component-bearing input cannot enter a plain recipe"
                );
                assert_eq!(peer.inventory_count(2), 4);
                activate(peer.session_mut(), "recipe:browser/choose_gravel");
                change(peer.session_mut(), "recipe:browser/quantity", "3");
                let notes = peer
                    .session_mut()
                    .node_index("recipe:browser/notes")
                    .unwrap();
                assert_eq!(
                    peer.session_mut().text_at(notes),
                    "Local recipe notes\nKeep exact input components",
                    "dynamic row replacement preserves unrelated multiline edits"
                );
                peer.select_slot(1);
                activate(peer.session_mut(), "recipe:browser/craft");
                let reverse = peer.submit_ui_action();
                peer.resend(&reverse);
                assert!(peer.result(&reverse).0);
                assert!(peer.result(&reverse).0);
                assert_eq!(peer.inventory_count(0), 8);
                assert_eq!(peer.inventory_count(1), 125);
            } else {
                assert_eq!(peer.inventory_count(0), 8);
                assert_eq!(peer.inventory_count(1), 125);
                assert_eq!(peer.inventory_count(2), 4);
            }
        });
        let recovered = open(&fixture);
        let inventory = recovered.inventory_store.load(PROFILE).unwrap();
        expected_revision = inventory.revision;
        assert_eq!(inventory.slots[0].as_ref().unwrap().count, 8);
        assert_eq!(inventory.slots[1].as_ref().unwrap().count, 125);
        assert_eq!(
            inventory.slots[2],
            Stack::with_components(stone, 4, 1, vec![0, 255])
        );
        assert_eq!(
            inventory
                .slots
                .iter()
                .flatten()
                .filter(|stack| stack.item == stone || stack.item == gravel)
                .map(|stack| stack.count)
                .sum::<u16>(),
            137
        );
        assert!(
            inventory
                .slots
                .iter()
                .flatten()
                .all(|stack| stack.count <= 128)
        );
    }
}

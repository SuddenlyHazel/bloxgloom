use super::*;
use crate::appearance::{AppearanceState, CharacterRecipe};

impl Peer {
    fn character_state(&mut self, id: u64, expected: AppearanceState) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self
            .views
            .get(&id)
            .is_none_or(|view| AppearanceState::decode(&view.payload) != Some(expected))
        {
            self.read(deadline);
        }
        self.views[&id].revision
    }
}

#[test]
fn articulated_recipes_replicate_independently_and_survive_server_restart() {
    let fixture = Fixture::new();
    fixture.package("demo", CONTENT, &source(""));
    let first_recipe = CharacterRecipe {
        body: 1,
        hair_color: [66, 136, 206],
        eyes: 0,
        mouth: 0,
        hair: 13,
        iris: Some([12, 170, 255]),
    };
    let second_recipe = CharacterRecipe {
        body: 0,
        hair_color: [219, 184, 233],
        eyes: 0,
        mouth: 0,
        hair: 0,
        iris: None,
    };
    let first_saved = AppearanceState {
        packaged: None,
        palettes: [6, 8, 6],
        character: Some(first_recipe),
    };
    let second_saved = AppearanceState {
        packaged: None,
        palettes: [0; 3],
        character: Some(second_recipe),
    };
    for restarted in [false, true] {
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        gameplay::serve(state, |address| {
            let mut first = Peer::connect(address, 0xa991, &catalog);
            let a = first.own;
            if !restarted {
                first.character_state(a, AppearanceState::default());
                first.send(ClientMessage::SelectCharacter {
                    recipe: Some(first_recipe),
                });
                first.character_state(
                    a,
                    AppearanceState {
                        packaged: None,
                        character: Some(first_recipe),
                        ..Default::default()
                    },
                );
                first.send(ClientMessage::SelectAppearance {
                    palettes: first_saved.palettes,
                });
            }
            let revision = first.character_state(a, first_saved);
            // The late join sees the already-committed character immediately.
            let mut second = Peer::connect(address, 0xa992, &catalog);
            let b = second.own;
            second.character_state(a, first_saved);
            if !restarted {
                second.send(ClientMessage::SelectCharacter {
                    recipe: Some(second_recipe),
                });
            }
            second.character_state(b, second_saved);
            first.character_state(b, second_saved);
            let saved = fixture
                .0
                .join("save/players/0000000000000000000000000000a991.appearance");
            let bytes = std::fs::read(&saved).unwrap();
            first.send(ClientMessage::SelectCharacter {
                recipe: Some(first_recipe),
            });
            first.barrier();
            assert_eq!(first.views[&a].revision, revision);
            assert_eq!(std::fs::read(saved).unwrap(), bytes);
            assert_eq!(
                AppearanceState::decode(&first.views[&b].payload),
                Some(second_saved)
            );
            if restarted {
                first.send(ClientMessage::SelectCharacter { recipe: None });
                let default_character = AppearanceState {
                    packaged: None,
                    palettes: first_saved.palettes,
                    character: None,
                };
                first.character_state(a, default_character);
                second.character_state(a, default_character);
                assert_eq!(
                    AppearanceState::decode(&second.views[&b].payload),
                    Some(second_saved)
                );
            }
        });
    }
}

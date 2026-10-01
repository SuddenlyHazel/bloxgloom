use super::*;
use crate::appearance::{AppearanceState, CharacterRecipe};

#[test]
fn recipe_save_failure_never_publishes_and_invalid_selection_never_writes() {
    let save = TestSave::new("character-save-failure");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 0xa996);
    let path = save
        .path()
        .join("players/0000000000000000000000000000a996.appearance");
    let before = state
        .player_entities
        .appearance_state_for_session(session.id)
        .unwrap();
    assert!(
        handle_message(
            &mut state,
            session.id,
            ClientMessage::SelectCharacter {
                recipe: Some(CharacterRecipe {
                    hair: 3,
                    ..Default::default()
                })
            }
        )
        .is_err()
    );
    assert!(!path.exists());
    assert!(!state.durability.failed);
    std::fs::create_dir(&path).unwrap();
    assert!(
        handle_message(
            &mut state,
            session.id,
            ClientMessage::SelectCharacter {
                recipe: Some(CharacterRecipe::default())
            }
        )
        .is_err()
    );
    assert!(state.durability.failed);
    assert_eq!(
        state
            .player_entities
            .appearance_state_for_session(session.id),
        Some(before)
    );
}

#[test]
fn recipe_save_is_profile_bound_canonical_and_exposes_only_legacy_host_projection() {
    let save = TestSave::new("character-save-roundtrip");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 0xa997);
    let recipe = CharacterRecipe {
        eyes: 7,
        mouth: 5,
        hair: 2,
        iris: Some([0, 255, 81]),
    };
    handle_message(
        &mut state,
        session.id,
        ClientMessage::SelectCharacter {
            recipe: Some(recipe),
        },
    )
    .unwrap();
    handle_message(
        &mut state,
        session.id,
        ClientMessage::SelectAppearance {
            palettes: [1, 2, 3],
        },
    )
    .unwrap();
    let expected = AppearanceState {
        palettes: [1, 2, 3],
        character: Some(recipe),
    };
    assert_eq!(
        state
            .appearance_store
            .load(0xa997, state.world.catalog())
            .unwrap(),
        expected
    );
    assert_eq!(
        state.player_entities.appearance_for_session(session.id),
        Some([1, 2, 3, 0])
    );
    assert_eq!(
        crate::server::players::capture(&state)[0].appearance,
        [1, 2, 3, 0]
    );
    let path = save
        .path()
        .join("players/0000000000000000000000000000a997.appearance");
    let original = std::fs::read(&path).unwrap();
    assert_eq!(&original[..4], b"BGA2");
    for bad in [
        original[..original.len() - 1].to_vec(),
        {
            let mut b = original.clone();
            b.push(0);
            b
        },
        {
            let mut b = original.clone();
            b[4] ^= 1;
            b
        },
        {
            let mut b = original.clone();
            b[3] = b'1';
            b
        },
        {
            let mut b = original.clone();
            b[25] ^= 1;
            b
        },
    ] {
        std::fs::write(&path, bad).unwrap();
        assert!(
            state
                .appearance_store
                .load(0xa997, state.world.catalog())
                .is_err()
        );
    }
    std::fs::write(&path, original).unwrap();
    assert_eq!(
        state
            .appearance_store
            .load(0xa997, state.world.catalog())
            .unwrap(),
        expected
    );
}

#[test]
fn profile_appearance_save_failure_never_publishes_and_stops_mutation() {
    let save = TestSave::new("appearance-save-failure");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 0xa993);
    let key = state.clients[&session.id].center;
    let before = state
        .player_entities
        .public_views_for_chunk_bounded(key, 2)
        .unwrap();
    let path = save
        .path()
        .join("players/0000000000000000000000000000a993.appearance");
    // Atomic replacement must fail, without a flaky worker/storage race.
    std::fs::create_dir(&path).unwrap();
    assert!(
        handle_message(
            &mut state,
            session.id,
            ClientMessage::SelectAppearance {
                palettes: [1, 2, 3]
            }
        )
        .is_err()
    );
    assert!(state.durability.failed);
    let after = state
        .player_entities
        .public_views_for_chunk_bounded(key, 2)
        .unwrap();
    assert_eq!(before, after);
    assert!(path.is_dir());
    assert!(
        handle_message(
            &mut state,
            session.id,
            ClientMessage::SelectAppearance {
                palettes: [0, 0, 0]
            }
        )
        .is_err()
    );
}

#[test]
fn profile_appearance_corruption_fails_closed_and_missing_profile_keeps_default() {
    let save = TestSave::new("appearance-corrupt");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 0xa994);
    handle_message(
        &mut state,
        session.id,
        ClientMessage::SelectAppearance {
            palettes: [1, 2, 3],
        },
    )
    .unwrap();
    let path = save
        .path()
        .join("players/0000000000000000000000000000a994.appearance");
    let original = std::fs::read(&path).unwrap();
    let key = state.clients[&session.id].center;
    let selected = state
        .player_entities
        .public_views_for_chunk_bounded(key, 2)
        .unwrap();
    handle_message(
        &mut state,
        session.id,
        ClientMessage::SelectAppearance {
            palettes: [1, 2, 3],
        },
    )
    .unwrap();
    assert_eq!(
        state
            .player_entities
            .public_views_for_chunk_bounded(key, 2)
            .unwrap(),
        selected,
        "identical retry must not increment revision"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(
        state
            .appearance_store
            .load(0xa994, state.world.catalog())
            .unwrap()
            .legacy(),
        [1, 2, 3, 0]
    );
    assert_eq!(
        state
            .appearance_store
            .load(0xa995, state.world.catalog())
            .unwrap()
            .legacy(),
        [0; 4]
    );
    for bad in [
        original[..original.len() - 1].to_vec(),
        {
            let mut bytes = original.clone();
            bytes[20] ^= 1;
            bytes
        },
        {
            let mut bytes = original.clone();
            bytes.push(0);
            bytes
        },
    ] {
        std::fs::write(&path, bad).unwrap();
        assert!(
            state
                .appearance_store
                .load(0xa994, state.world.catalog())
                .is_err()
        );
    }
    std::fs::write(&path, original).unwrap();
    assert_eq!(
        state
            .appearance_store
            .load(0xa994, state.world.catalog())
            .unwrap()
            .legacy(),
        [1, 2, 3, 0]
    );
}

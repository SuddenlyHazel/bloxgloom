use super::*;

#[test]
fn recipe_ids_and_optional_iris_round_trip_canonically() {
    for hair in 0..HAIR.len() as u8 {
        for eyes in 0..EYES.len() as u8 {
            for mouth in 0..MOUTHS.len() as u8 {
                for iris in [None, Some([0, 0, 0]), Some([255, 128, 1])] {
                    let recipe = CharacterRecipe {
                        hair,
                        eyes,
                        mouth,
                        iris,
                    };
                    assert!(recipe.valid());
                    assert_eq!(CharacterRecipe::decode(&recipe.encode()), Some(recipe));
                    let state = AppearanceState {
                        palettes: [1, 2, 3],
                        character: Some(recipe),
                    };
                    assert_eq!(state.encode().len(), MAX_APPEARANCE_BYTES);
                    assert_eq!(AppearanceState::decode(&state.encode()), Some(state));
                    assert_eq!(state.legacy(), [1, 2, 3, 0]);
                }
            }
        }
    }
    assert_eq!(
        CharacterRecipe::default(),
        CharacterRecipe {
            hair: 1,
            eyes: 1,
            mouth: 1,
            iris: None
        }
    );
    assert_eq!(AppearanceState::default().encode(), [0; 4]);
    assert_eq!(
        AppearanceState::decode(&[1, 2, 3, 0]).unwrap().palettes,
        [1, 2, 3]
    );
}

#[test]
fn malformed_recipe_or_public_payload_fails_closed() {
    let original = CharacterRecipe::default().encode();
    for (index, value) in [(0, 2), (1, 3), (2, 8), (3, 6), (4, 2), (5, 1)] {
        let mut bytes = original;
        bytes[index] = value;
        assert!(CharacterRecipe::decode(&bytes).is_none());
    }
    assert!(CharacterRecipe::decode(&original[..7]).is_none());
    assert!(CharacterRecipe::decode(&[0; 9]).is_none());
    for bytes in [
        vec![],
        vec![0; 3],
        vec![0; 5],
        vec![0; 11],
        vec![0; 13],
        vec![0, 0, 0, 1],
    ] {
        assert!(AppearanceState::decode(&bytes).is_none());
    }
}

#[test]
fn builtin_identity_is_stable_and_covers_exact_assets() {
    assert_eq!(fingerprint(), fingerprint());
    assert_ne!(*fingerprint(), [0; 32]);
    let catalog = crate::content::Catalog::builtins();
    assert_eq!(
        catalog
            .entity_type(crate::content::EntityTypeId(2))
            .unwrap()
            .schema_version,
        2
    );
    assert!(!catalog.valid_appearance_state(AppearanceState {
        palettes: [255, 0, 0],
        ..Default::default()
    }));
    assert!(!catalog.valid_appearance_state(AppearanceState {
        character: Some(CharacterRecipe {
            hair: 255,
            ..Default::default()
        }),
        ..Default::default()
    }));
}

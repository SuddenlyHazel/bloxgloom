use super::*;
use crate::appearance::CharacterRecipe;

#[test]
fn character_selection_is_session_scoped_and_bounded_on_wire() {
    for recipe in [
        None,
        Some(CharacterRecipe::default()),
        Some(CharacterRecipe {
            body: 1,
            hair_color: [66, 136, 206],
            hair: 13,
            eyes: 0,
            mouth: 0,
            iris: Some([0, 128, 255]),
        }),
    ] {
        let message = ClientMessage::SelectCharacter { recipe };
        let mut bytes = Vec::new();
        write_client(&mut bytes, &message).unwrap();
        assert_eq!(read_client(bytes.as_slice()).unwrap(), message);
        assert_eq!(
            bytes.len(),
            4 + 3 + recipe.map_or(0, |_| crate::appearance::CHARACTER_RECIPE_BYTES)
        );
    }
    let invalid_recipe = CharacterRecipe {
        body: 1,
        hair_color: [66, 136, 206],
        eyes: 8,
        ..Default::default()
    };
    assert!(
        write_client(
            Vec::new(),
            &ClientMessage::SelectCharacter {
                recipe: Some(invalid_recipe)
            }
        )
        .is_err()
    );
    let valid = CharacterRecipe::default().encode();
    let mut invalid_payloads = vec![
        vec![WIRE_VERSION, 19, 2],
        vec![WIRE_VERSION, 19, 0, 1],
        vec![WIRE_VERSION, 19, 1],
        // Old recipe v1 is not a truncated/new default recipe.
        vec![WIRE_VERSION, 19, 1, 1, 1, 1, 1, 0, 0, 0, 0],
    ];
    for (index, value) in [(0, 1), (1, 2), (2, 14), (3, 1), (4, 1), (5, 2), (6, 1)] {
        let mut recipe = valid;
        recipe[index] = value;
        let mut payload = vec![WIRE_VERSION, 19, 1];
        payload.extend(recipe);
        invalid_payloads.push(payload);
    }
    let mut old_wire = vec![20, 19, 1];
    old_wire.extend(valid);
    invalid_payloads.push(old_wire);
    let mut extra_byte = vec![WIRE_VERSION, 19, 1];
    extra_byte.extend(valid);
    extra_byte.push(0);
    invalid_payloads.push(extra_byte);
    for payload in invalid_payloads {
        let mut framed = Vec::new();
        frame(&mut framed, &payload).unwrap();
        assert!(read_client(framed.as_slice()).is_err());
    }
}

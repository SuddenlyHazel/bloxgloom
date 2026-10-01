use super::*;
use crate::appearance::CharacterRecipe;

#[test]
fn character_selection_is_session_scoped_and_bounded_on_wire() {
    for recipe in [
        None,
        Some(CharacterRecipe::default()),
        Some(CharacterRecipe {
            hair: 2,
            eyes: 7,
            mouth: 5,
            iris: Some([0, 128, 255]),
        }),
    ] {
        let message = ClientMessage::SelectCharacter { recipe };
        let mut bytes = Vec::new();
        write_client(&mut bytes, &message).unwrap();
        assert_eq!(read_client(bytes.as_slice()).unwrap(), message);
    }
    let invalid_recipe = CharacterRecipe {
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
    for payload in [
        vec![WIRE_VERSION, 19, 2],
        vec![WIRE_VERSION, 19, 0, 1],
        vec![WIRE_VERSION, 19, 1],
        vec![WIRE_VERSION, 19, 1, 1, 3, 0, 0, 0, 0, 0, 0],
        vec![WIRE_VERSION, 19, 1, 1, 1, 0, 0, 0, 1, 0, 0],
    ] {
        let mut framed = Vec::new();
        frame(&mut framed, &payload).unwrap();
        assert!(read_client(framed.as_slice()).is_err());
    }
}

use super::*;
#[test]
fn selected_hand_light_uses_nonempty_authoritative_stack_and_block_emission_only() {
    let catalog = crate::content::Catalog::builtins();
    let mut inventory = Inventory::default();
    let item = crate::content::ItemId(crate::world::GLOWSTONE.0);
    assert_eq!(selected_emission(&inventory, 0, &catalog), 0);
    inventory.slots[0] = Some(crate::inventory::Stack::new(item, 1));
    assert_eq!(
        selected_emission(&inventory, 0, &catalog),
        catalog.emission(crate::world::GLOWSTONE)
    );
    assert!(selected_emission(&inventory, 0, &catalog) > 0);
    assert_eq!(selected_emission(&inventory, 1, &catalog), 0);
    assert_eq!(selected_emission(&inventory, usize::MAX, &catalog), 0);
    inventory.slots[0].as_mut().unwrap().count = 0;
    assert_eq!(selected_emission(&inventory, 0, &catalog), 0);
    inventory.slots[0] = Some(crate::inventory::Stack::new(crate::items::STICK, 1));
    assert_eq!(selected_emission(&inventory, 0, &catalog), 0);
    inventory.slots[0] = Some(crate::inventory::Stack::new(
        crate::content::ItemId(crate::world::STONE.0),
        1,
    ));
    assert_eq!(selected_emission(&inventory, 0, &catalog), 0);
    assert_eq!(
        inventory.slots[0].as_ref().unwrap().count,
        1,
        "presentation must not consume inventory"
    );
}

#[test]
fn iris_relative_eye_offset_recovers_player_space_from_third_person_camera() {
    let player = Vec3::new(-2015.0, 30.0, 512.0);
    let world = Vec3::new(-2013.0, 28.0, 510.0);
    for view in [
        player,
        player + Vec3::new(4.0, 1.0, -3.0),
        player - Vec3::new(4.0, 1.0, -3.0),
    ] {
        let offset = Vec3::from_array(relative_eye(view, player));
        assert_eq!(world - view + offset, world - player);
    }
    assert_eq!(relative_eye(player, player), [0.0; 3]);
}

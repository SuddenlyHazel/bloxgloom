use super::*;
use crate::raycast::Face;

#[test]
fn escape_and_inventory_transitions_preserve_menu_flow() {
    assert_eq!(escape_screen(UiScreen::Playing), UiScreen::Pause);
    assert_eq!(escape_screen(UiScreen::Pause), UiScreen::Playing);
    assert_eq!(escape_screen(UiScreen::Settings), UiScreen::Pause);
    assert_eq!(escape_screen(UiScreen::Inventory), UiScreen::Playing);
    assert_eq!(inventory_screen(UiScreen::Playing), UiScreen::Inventory);
    assert_eq!(inventory_screen(UiScreen::Inventory), UiScreen::Playing);
    assert_eq!(inventory_screen(UiScreen::Pause), UiScreen::Pause);
}

#[test]
fn block_edit_uses_selected_hotbar_block_and_hit_face() {
    let hit = Hit {
        block: [2, 3, 4],
        adjacent: [1, 3, 4],
        block_id: 3,
        distance: 2.5,
        face: Face::NegX,
    };
    assert_eq!(
        edit_for_hit(hit, true, Some(1), 2),
        Some(ClientMessage::Edit {
            x: 1,
            y: 3,
            z: 4,
            block: 1,
            slot: 2,
        })
    );
    assert_eq!(
        edit_for_hit(hit, false, Some(1), 2),
        Some(ClientMessage::Edit {
            x: 2,
            y: 3,
            z: 4,
            block: 0,
            slot: 2,
        })
    );
}

#[test]
fn placing_on_a_replaceable_flower_targets_its_cell() {
    let hit = Hit {
        block: [2, 3, 4],
        adjacent: [2, 4, 4],
        block_id: crate::world::RED_FLOWER,
        distance: 2.5,
        face: Face::PosY,
    };
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::world::WOOD), 0),
        Some(ClientMessage::Edit {
            x: 2,
            y: 3,
            z: 4,
            block: crate::world::WOOD,
            slot: 0,
        })
    );
    assert_eq!(edit_for_hit(hit, true, Some(crate::items::SEEDS), 0), None);
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::items::SAPLING), 0),
        None
    );
    assert_eq!(edit_for_hit(hit, true, Some(crate::items::STICK), 0), None);
    assert!(edit_for_hit(hit, false, Some(crate::items::SEEDS), 0).is_some());
}

#[test]
fn client_cache_uses_server_view_radius() {
    let center = ChunkKey { x: -10, y: 4, z: 5 };
    assert!(chunk_in_view(
        ChunkKey {
            x: -16,
            y: 5,
            z: 11
        },
        center,
        6
    ));
    assert!(!chunk_in_view(
        ChunkKey {
            x: -16,
            y: 5,
            z: 11
        },
        center,
        3
    ));
    assert!(!chunk_in_view(ChunkKey { x: -10, y: 6, z: 5 }, center, 6));
}

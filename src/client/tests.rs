use super::*;
use crate::raycast::Face;

#[test]
fn skylight_capture_and_invalidation_include_distant_roofs() {
    let target = ChunkKey {
        x: 16,
        y: -1,
        z: 20,
    };
    assert!(lighting_depends_on(target, ChunkKey { y: 4, ..target }));
    assert!(lighting_depends_on(target, ChunkKey { y: -2, ..target }));
    assert!(!lighting_depends_on(target, ChunkKey { y: -3, ..target }));
    assert!(!lighting_depends_on(
        target,
        ChunkKey {
            x: 18,
            y: 4,
            ..target
        }
    ));
}

#[test]
fn missing_meshes_are_nearest_first_without_delaying_urgent_edits() {
    let center = ChunkKey { x: 0, y: 0, z: 0 };
    let near = ChunkKey { x: 1, ..center };
    let far = ChunkKey { x: 5, ..center };
    assert!(mesh_priority(near, center, false, false) < mesh_priority(far, center, false, false));
    assert!(mesh_priority(far, center, false, false) < mesh_priority(near, center, false, true));
    assert!(mesh_priority(far, center, true, true) < mesh_priority(near, center, false, false));
}

#[test]
fn client_retains_the_expanded_vertical_band() {
    let center = ChunkKey { x: -2, y: -3, z: 1 };
    for offset in [-4, 4] {
        assert!(chunk_in_view(
            ChunkKey {
                y: center.y + offset,
                ..center
            },
            center,
            1
        ));
    }
    for offset in [-5, 5] {
        assert!(!chunk_in_view(
            ChunkKey {
                y: center.y + offset,
                ..center
            },
            center,
            1
        ));
    }
}

#[test]
fn action_ids_keep_session_and_order_without_reuse() {
    let first = action_id(0x1234, 1);
    let second = action_id(0x1234, 2);
    let other_session = action_id(0x1235, 1);
    assert_eq!(first >> 64, 0x1234);
    assert_eq!(first as u64, 1);
    assert!(first < second);
    assert_ne!(first, other_session);
}

#[test]
fn action_results_ack_only_a_contiguous_server_issued_session() {
    let mut tracker = ActionTracker::default();
    assert!(tracker.allocate().is_none());
    assert!(tracker.install_fresh_session(17, 1, 0).is_ok());
    assert!(tracker.install_fresh_session(18, 1, 0).is_err());
    let first = tracker.allocate().unwrap();
    let second = tracker.allocate().unwrap();
    assert_eq!(first, action_id(17, 1));
    assert_eq!(second, action_id(17, 2));
    assert_eq!(tracker.terminal_result(second).unwrap(), None);
    assert_eq!(tracker.terminal_result(first).unwrap(), Some(2));
    assert_eq!(tracker.terminal_result(first).unwrap(), None);
    assert!(tracker.terminal_result(action_id(16, 1)).is_err());
    assert!(tracker.terminal_result(action_id(17, 3)).is_err());
}

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
        block_id: crate::world::BlockId::new(3),
        distance: 2.5,
        face: Face::NegX,
    };
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::items::ItemId::new(1)), 2, 71),
        Some(ClientMessage::Edit {
            action_id: 71,
            x: 1,
            y: 3,
            z: 4,
            block: crate::world::BlockId::new(1),
            slot: 2,
        })
    );
    assert_eq!(
        edit_for_hit(hit, false, Some(crate::items::ItemId::new(1)), 2, 72),
        Some(ClientMessage::Edit {
            action_id: 72,
            x: 2,
            y: 3,
            z: 4,
            block: crate::world::AIR,
            slot: 2,
        })
    );
}

#[test]
fn cardinal_placement_hint_follows_camera_yaw() {
    let catalog = crate::content::catalog();
    let hit = Hit {
        block: [2, 3, 4],
        adjacent: [2, 4, 4],
        block_id: crate::world::STONE,
        distance: 2.5,
        face: Face::PosY,
    };
    for (yaw, facing) in [
        (0.0, "west"),
        (std::f32::consts::FRAC_PI_2, "north"),
        (std::f32::consts::PI, "east"),
        (-std::f32::consts::FRAC_PI_2, "south"),
    ] {
        let message = edit_for_hit_with_catalog(
            hit,
            true,
            Some(crate::content::KILN_ITEM),
            0,
            91,
            yaw,
            catalog,
        )
        .unwrap();
        let ClientMessage::Edit { block, .. } = message else {
            panic!("placement did not produce an edit");
        };
        assert_eq!(
            block,
            catalog
                .state_with_property(crate::content::KILN_DEFAULT_STATE, "facing", facing)
                .unwrap()
        );
    }
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
        edit_for_hit(
            hit,
            true,
            Some(crate::items::ItemId::new(crate::world::WOOD.get())),
            0,
            73
        ),
        Some(ClientMessage::Edit {
            action_id: 73,
            x: 2,
            y: 3,
            z: 4,
            block: crate::world::WOOD,
            slot: 0,
        })
    );
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::items::SEEDS), 0, 74),
        None
    );
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::items::SAPLING), 0, 75),
        None
    );
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::items::STICK), 0, 76),
        None
    );
    assert!(edit_for_hit(hit, false, Some(crate::items::SEEDS), 0, 77).is_some());
}

#[test]
fn mapped_server_item_and_replaceable_state_drive_placement_preview() {
    use crate::content::{BlockStateId, ContentManifest};
    use crate::items::ItemId;
    use glam::Vec3;

    let local = crate::content::Catalog::builtins();
    let mut manifest = ContentManifest::from_catalog(&local);
    for entry in &mut manifest.entries {
        match (entry.kind, entry.key.as_str()) {
            (b'B', "bloxgloom:red_flower") => entry.id = 65_536,
            (b'S', "bloxgloom:red_flower") => entry.id = 65_537,
            (b'I', "bloxgloom:red_flower") => entry.id = 65_538,
            _ => {}
        }
    }
    manifest
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    let catalog = manifest.resolve_catalog(&local).unwrap();
    let flower = BlockStateId::new(65_537);
    let item = ItemId::new(65_538);
    assert!(catalog.state(crate::world::RED_FLOWER).is_none());

    let hit = raycast::raycast_with_catalog(
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::X,
        7.0,
        |x, y, z| {
            Some(if x == 2 && y == 0 && z == 0 {
                flower
            } else {
                crate::world::AIR
            })
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(hit.block_id, flower);
    assert_eq!(
        edit_for_hit_with_catalog(hit, true, Some(item), 0, 91, 0.0, &catalog),
        Some(ClientMessage::Edit {
            action_id: 91,
            x: 2,
            y: 0,
            z: 0,
            block: flower,
            slot: 0,
        })
    );
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
    assert!(!chunk_in_view(ChunkKey { x: -10, y: 9, z: 5 }, center, 6));
}

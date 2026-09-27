//! Behavior tests for UI layout and bounded geometry generation.
use crate::content::Catalog;

#[test]
fn maximum_registered_container_layout_and_geometry_stay_bounded() {
    let catalog = Catalog::builtins();
    let screen = std::sync::Arc::new(bloxgloom_host_api::InventoryScreen::storage(
        "test:large",
        "test:large",
        "LARGE CONTAINER",
        54,
        9,
        vec![[0; 3]],
    ));
    screen.validate().unwrap();
    for (width, height, scale) in [(640, 360, 1.0), (640, 360, 1.8), (1280, 720, 1.8)] {
        let layout =
            UiLayout::new(width, height, scale, UiScreen::Container).with_container(Some(&screen));
        let stack = crate::inventory::Stack::new(crate::items::STICK, 128);
        let frame = UiFrame {
            screen: UiScreen::Container,
            container_screen: Some(screen.clone()),
            kiln: Some(crate::protocol::workstation::WorkstationView {
                slots: vec![Some(stack.clone()); 54],
                status: vec![],
            }),
            inventory: std::array::from_fn(|_| Some(stack.clone())),
            ..Default::default()
        };
        for control in (0..54)
            .map(UiControl::KilnSlot)
            .chain((0..36).map(UiControl::InventorySlot))
        {
            let rect = layout.rect(control).unwrap();
            assert!(
                rect.x >= 0.0
                    && rect.y >= 0.0
                    && rect.x + rect.width <= width as f32
                    && rect.y + rect.height <= height as f32
            );
            assert_eq!(
                layout.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
                Some(control)
            );
        }
        let mut vertices = Vec::new();
        let mut builder = UiBuilder {
            vertices: &mut vertices,
            width: width as f32,
            height: height as f32,
            scale: layout.scale,
        };
        builder.draw_frame(&frame, &layout, &catalog);
        assert!(vertices.len() <= MAX_UI_VERTICES);
    }
}

use super::{
    draw::{MAX_UI_VERTICES, UiBuilder, item_color, item_name},
    layout::{UiLayout, effective_ui_scale},
    types::{SettingId, UiControl, UiDebug, UiFrame, UiScreen, UiSettings},
};

#[test]
fn kiln_controls_fit_and_hit_test_on_compact_and_large_screens() {
    for (width, height, scale) in [(640, 360, 1.0), (1280, 720, 1.0), (640, 360, 1.5)] {
        let catalog = Catalog::builtins();
        let layout = UiLayout::new(width, height, scale, UiScreen::Container).with_container(
            catalog
                .inventory_screen(crate::content::KILN_ENTITY_TYPE)
                .map(|s| s.as_ref()),
        );
        for control in (0..3)
            .map(UiControl::KilnSlot)
            .chain((0..36).map(UiControl::InventorySlot))
        {
            let rect = layout.rect(control).unwrap();
            assert!(rect.x >= 0.0 && rect.y >= 0.0);
            assert!(rect.x + rect.width <= width as f32 && rect.y + rect.height <= height as f32);
            assert_eq!(
                layout.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
                Some(control)
            );
        }
    }
}

#[test]
fn chest_slots_and_backpack_are_distinct_and_usable_at_small_and_large_sizes() {
    for (width, height, scale) in [
        (640, 360, 1.0),
        (640, 360, 1.8),
        (1280, 720, 1.0),
        (1280, 720, 1.8),
    ] {
        let catalog = Catalog::builtins();
        let layout = UiLayout::new(width, height, scale, UiScreen::Container).with_container(
            catalog
                .inventory_screen(crate::content::CHEST_ENTITY_TYPE)
                .map(|s| s.as_ref()),
        );
        let mut rects = Vec::new();
        for control in (0..27)
            .map(UiControl::KilnSlot)
            .chain((0..36).map(UiControl::InventorySlot))
        {
            let rect = layout.rect(control).unwrap();
            assert!(
                rect.x >= 0.0
                    && rect.y >= 0.0
                    && rect.x + rect.width <= width as f32
                    && rect.y + rect.height <= height as f32
            );
            assert_eq!(
                layout.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
                Some(control)
            );
            for old in &rects {
                let old: &super::types::UiRect = old;
                assert!(
                    rect.x >= old.x + old.width
                        || old.x >= rect.x + rect.width
                        || rect.y >= old.y + old.height
                        || old.y >= rect.y + rect.height
                );
            }
            rects.push(rect);
        }
    }
}

#[test]
fn admin_catalog_controls_fit_compact_and_desktop_panels() {
    for (width, height, scale) in [(1280, 720, 1.0), (640, 360, 1.0), (640, 360, 2.0)] {
        let layout = UiLayout::new(width, height, scale, UiScreen::Admin);
        let panel = layout.admin_panel();
        for index in 0..24 {
            let rect = layout.rect(UiControl::AdminItem(index)).unwrap();
            assert!(rect.x >= panel.x && rect.y >= panel.y);
            assert!(rect.x + rect.width <= panel.x + panel.width);
            assert!(rect.y + rect.height < panel.y + panel.height - 90.0 * layout.scale);
        }
        for control in [
            UiControl::AdminPrev,
            UiControl::AdminNext,
            UiControl::AdminRun,
        ] {
            let rect = layout.rect(control).unwrap();
            assert!(rect.x >= panel.x && rect.x + rect.width <= panel.x + panel.width);
            assert!(rect.y >= panel.y && rect.y + rect.height <= panel.y + panel.height);
        }
    }
}

#[test]
fn world_and_inventory_items_have_distinct_hud_names_and_swatch_colors() {
    let vegetation = [
        (
            crate::items::ItemId::new(9),
            "WOOD",
            [0.55, 0.34, 0.19, 1.0],
        ),
        (
            crate::items::ItemId::new(10),
            "LEAVES",
            [0.30, 0.62, 0.34, 1.0],
        ),
        (
            crate::items::ItemId::new(11),
            "RED FLOWER",
            [0.86, 0.20, 0.29, 1.0],
        ),
        (
            crate::items::ItemId::new(12),
            "YELLOW FLOWER",
            [0.96, 0.68, 0.14, 1.0],
        ),
        (
            crate::items::ItemId::new(13),
            "BLUE FLOWER",
            [0.33, 0.56, 0.88, 1.0],
        ),
        (
            crate::items::ItemId::new(14),
            "FERN",
            [0.34, 0.66, 0.37, 1.0],
        ),
        (
            crate::items::ItemId::new(15),
            "TALL GRASS",
            [0.38, 0.69, 0.34, 1.0],
        ),
        (crate::items::SEEDS, "SEEDS", [0.77, 0.52, 0.27, 1.0]),
        (crate::items::SAPLING, "SAPLING", [0.33, 0.65, 0.38, 1.0]),
        (crate::items::STICK, "STICK", [0.61, 0.39, 0.22, 1.0]),
    ];
    for (item, name, color) in vegetation {
        assert_eq!(item_name(item), name);
        assert_eq!(item_color(item), color);
    }
}

#[test]
fn remapped_inventory_item_uses_connection_name_color_and_builtin_art() {
    use crate::content::ContentManifest;
    use crate::items::ItemId;

    let local = crate::content::Catalog::builtins();
    let mut manifest = ContentManifest::from_catalog(&local);
    for entry in &mut manifest.entries {
        if entry.kind == b'I' && entry.key == "bloxgloom:red_flower" {
            entry.id = 65_538;
        }
    }
    manifest
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    let catalog = manifest.resolve_catalog(&local).unwrap();
    let item = ItemId::new(65_538);

    assert_eq!(super::draw::item_name_for(item, &catalog), "RED FLOWER");
    assert_eq!(
        super::draw::item_color_for(item, &catalog),
        [0.86, 0.20, 0.29, 1.0]
    );
    let mut vertices = Vec::new();
    let mut builder = UiBuilder {
        vertices: &mut vertices,
        width: 1280.0,
        height: 720.0,
        scale: 1.0,
    };
    builder.draw_item_swatch(
        super::types::UiRect {
            x: 10.0,
            y: 10.0,
            width: 64.0,
            height: 64.0,
        },
        item,
        &catalog,
    );
    assert!(
        vertices.len() > 6,
        "mapped builtin art should draw its pixel swatch"
    );
}

#[test]
fn hotbar_layout_hit_tests_nine_slots_without_gaps() {
    let layout = UiLayout::new(1280, 720, 1.0, UiScreen::Playing);
    for index in 0..9 {
        let rect = layout.rect(UiControl::HotbarSlot(index)).unwrap();
        assert_eq!(
            layout.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
            Some(UiControl::HotbarSlot(index))
        );
    }
    assert!(layout.rect(UiControl::HotbarSlot(9)).is_none());
}

#[test]
fn menu_layouts_expose_only_visible_actions() {
    let pause = UiLayout::new(640, 360, 1.0, UiScreen::Pause);
    assert!(pause.rect(UiControl::Resume).is_some());
    assert!(pause.rect(UiControl::OpenSettings).is_some());
    assert!(pause.rect(UiControl::Exit).is_some());
    assert!(pause.rect(UiControl::ToggleFullscreen).is_none());

    let inventory = UiLayout::new(1280, 720, 1.0, UiScreen::Inventory);
    for slot in 0..36 {
        assert!(inventory.rect(UiControl::InventorySlot(slot)).is_some());
    }
    let compact_inventory = UiLayout::new(640, 360, 1.0, UiScreen::Inventory);
    for slot in 0..36 {
        let card = compact_inventory
            .rect(UiControl::InventorySlot(slot))
            .unwrap();
        assert_eq!(
            compact_inventory.hit_test(card.x + card.width * 0.5, card.y + card.height * 0.5),
            Some(UiControl::InventorySlot(slot))
        );
    }
}

#[test]
fn settings_layout_has_adjusters_and_fullscreen_toggle() {
    let layout = UiLayout::new(1280, 720, 1.0, UiScreen::Settings);
    for setting in [
        SettingId::Sensitivity,
        SettingId::FieldOfView,
        SettingId::ViewDistance,
        SettingId::UiScale,
        SettingId::Lighting,
    ] {
        assert!(layout.rect(UiControl::Decrease(setting)).is_some());
        assert!(layout.rect(UiControl::Increase(setting)).is_some());
    }
    assert!(layout.rect(UiControl::ToggleFullscreen).is_some());
    assert!(layout.rect(UiControl::Back).is_some());
}

#[test]
fn graphics_controls_fit_and_hit_test_at_both_ui_scales() {
    for (width, height, scale) in [
        (640, 360, 1.0),
        (640, 360, 2.0),
        (1280, 720, 1.0),
        (1280, 720, 2.0),
    ] {
        let layout = UiLayout::new(width, height, scale, UiScreen::Graphics);
        let mut controls = vec![UiControl::ToggleSettingsPage, UiControl::Back];
        for setting in [
            SettingId::PostProcessing,
            SettingId::Exposure,
            SettingId::Bloom,
            SettingId::BloomStrength,
        ] {
            controls.extend([UiControl::Decrease(setting), UiControl::Increase(setting)]);
        }
        for control in controls {
            let rect = layout.rect(control).unwrap();
            assert!(
                rect.x >= 0.0
                    && rect.y >= 0.0
                    && rect.x + rect.width <= width as f32
                    && rect.y + rect.height <= height as f32
            );
            assert_eq!(
                layout.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
                Some(control)
            );
        }
        assert!(layout.rect(UiControl::ToggleFullscreen).is_none());
    }
}

#[test]
fn compact_controls_hit_test_at_their_visible_centers() {
    let inventory = UiLayout::new(640, 360, 1.0, UiScreen::Inventory);
    let card = inventory.rect(UiControl::InventorySlot(10)).unwrap();
    assert_eq!(
        inventory.hit_test(card.x + card.width * 0.5, card.y + card.height * 0.5),
        Some(UiControl::InventorySlot(10))
    );

    let settings = UiLayout::new(640, 360, 1.0, UiScreen::Settings);
    for control in [
        UiControl::Decrease(SettingId::Sensitivity),
        UiControl::Increase(SettingId::ViewDistance),
        UiControl::Increase(SettingId::Lighting),
        UiControl::ToggleFullscreen,
        UiControl::Back,
    ] {
        let rect = settings.rect(control).unwrap();
        assert_eq!(
            settings.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
            Some(control)
        );
    }

    let pause = UiLayout::new(640, 360, 1.0, UiScreen::Pause);
    for control in [UiControl::Resume, UiControl::OpenSettings, UiControl::Exit] {
        let rect = pause.rect(control).unwrap();
        assert_eq!(
            pause.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
            Some(control)
        );
    }
}

#[test]
fn large_ui_scale_fits_small_windows_without_losing_controls() {
    assert_eq!(effective_ui_scale(640, 360, 2.0), 1.0);
    assert_eq!(effective_ui_scale(1280, 720, 2.0), 2.0);
    for (width, height) in [(640, 360), (1280, 720)] {
        for screen in [
            UiScreen::Playing,
            UiScreen::Inventory,
            UiScreen::Admin,
            UiScreen::Pause,
            UiScreen::Settings,
            UiScreen::Graphics,
        ] {
            let layout = UiLayout::new(width, height, 2.0, screen);
            for hit in &layout.hits {
                assert!(hit.rect.x >= 0.0, "{screen:?}: {:?}", hit.control);
                assert!(hit.rect.y >= 0.0, "{screen:?}: {:?}", hit.control);
                assert!(
                    hit.rect.x + hit.rect.width <= width as f32,
                    "{screen:?}: {:?}",
                    hit.control
                );
                assert!(
                    hit.rect.y + hit.rect.height <= height as f32,
                    "{screen:?}: {:?}",
                    hit.control
                );
            }
        }
    }
}

#[test]
fn worst_case_ui_stays_well_within_fixed_vertex_budget() {
    let long_status = "CONNECTION MESSAGE THAT SHOULD BE CLIPPED BEFORE IT CAN GROW THE UI BUFFER";
    let debug = UiDebug {
        position: [1234.5, -12.0, 9876.25],
        fps: 60.0,
        frame_ms: 16.6,
        visible_chunks: 512,
        cached_chunks: 512,
        latency_ms: Some(250),
    };
    for (width, height, scale) in [(1280, 720, 2.0), (640, 360, 2.0)] {
        for screen in [
            UiScreen::Playing,
            UiScreen::Inventory,
            UiScreen::Pause,
            UiScreen::Settings,
            UiScreen::Graphics,
        ] {
            let frame = UiFrame {
                container_screen: None,
                screen,
                selected_slot: 8,
                inventory: std::array::from_fn(|_| None),
                inventory_source: None,
                kiln: None,
                kiln_source: None,
                admin_enabled: true,
                admin_page: 0,
                admin_input: "give bloxgloom:stone 128",
                target: Some([10, 20, -30]),
                status: Some(long_status),
                debug: Some(debug),
                settings: UiSettings {
                    scale,
                    ..UiSettings::default()
                },
                hovered: Some(UiControl::Increase(SettingId::FieldOfView)),
            };
            let layout = UiLayout::new(width, height, scale, screen);
            let mut vertices = Vec::with_capacity(MAX_UI_VERTICES);
            let mut builder = UiBuilder {
                vertices: &mut vertices,
                width: width as f32,
                height: height as f32,
                scale,
            };
            builder.draw_frame(&frame, &layout, &crate::content::Catalog::builtins());
            assert!(
                vertices.len() < MAX_UI_VERTICES / 2,
                "{screen:?} at {width}x{height}: {} vertices",
                vertices.len()
            );
        }
    }
}

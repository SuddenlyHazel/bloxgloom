use super::*;
use std::time::Duration;

fn app() -> ClientApp {
    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        std::env::temp_dir().join(format!("interaction-{}", std::process::id())),
    );
    app.pitch = 0.0;
    app.config.selected_slot = 0;
    app.grabbed = true;
    app.actions.epoch = 7;
    app.actions.next_seq = 1;
    let key = ChunkKey { x: 0, y: 2, z: 0 };
    let mut blocks = vec![AIR; crate::world::CHUNK_VOLUME];
    blocks[Chunk::index([3, 9, 0]).unwrap()] = crate::world::STONE;
    app.chunks
        .insert(key, Arc::new(Chunk::from_blocks(key, 1, blocks)));
    app.inventory.slots[0] = Some(crate::inventory::Stack::new(
        app.catalog.item_by_key("bloxgloom:stone").unwrap(),
        5,
    ));
    app
}

#[test]
fn right_click_places_selected_block_instead_of_opening_generic_actions() {
    let mut app = app();
    app.place_or_interact();
    assert_eq!(app.screen, UiScreen::Playing);
    assert!(app.action_choices.is_empty());
    assert!(app.pending_commands.iter().any(|message| matches!(message,ClientMessage::Edit{x:2,y:41,z:0,block,..} if *block == crate::world::STONE)), "queued requests: {:?}", app.pending_commands);
    assert_eq!(
        app.inventory.slots[0].as_ref().unwrap().count,
        5,
        "request does not create or debit items locally"
    );
    assert!(
        !app.open_item_actions(),
        "admin clock command is not an argument-less item action"
    );
}

#[test]
fn breaking_starts_a_bounded_tool_animation_and_stance_requires_server_confirmation() {
    let mut app = app();
    app.edit_aimed_block(false);
    let mut avatars = [avatar(&app)];
    let now = Instant::now();
    app.character_motion
        .apply(&mut avatars[0], now + Duration::from_millis(200));
    assert!(matches!(avatars[0].character_tool,Some((true,t)) if t>0.1 && t<0.5));
    app.character_motion
        .apply(&mut avatars[0], now + Duration::from_secs(2));
    assert_eq!(avatars[0].character_tool, None);
    app.owned_entity_id = Some(1);
    let standing_eye = app.camera().position;
    app.request_crouch(true);
    assert!(!app.crouching(), "request is not authoritative stance");
    app.accept(ServerMessage::PlayerStance {
        entity_id: 1,
        crouching: true,
    });
    assert!(app.crouching());
    assert!(app.camera().position.y < standing_eye.y);
    app.set_screen(UiScreen::Pause);
    assert!(!app.crouch_requested, "menus release held crouch");
    assert!(
        app.crouching(),
        "server keeps stance until it can safely stand"
    );
    app.accept(ServerMessage::PlayerStance {
        entity_id: 1,
        crouching: false,
    });
    assert_eq!(app.camera().position, standing_eye);
}

fn avatar(app: &ClientApp) -> crate::render::VisualAvatar {
    let entity = crate::protocol::PublicEntity {
        id: 1,
        entity_type: crate::content::EntityTypeId(2),
        revision: 1,
        motion_revision: 1,
        location: crate::protocol::PublicEntityLocation::Mobile {
            position: app.position.to_array(),
        },
        payload: crate::appearance::AppearanceState {
            palettes: [0; 3],
            character: Some(Default::default()),
        }
        .encode(),
    };
    app.entity_registry
        .project(&BTreeMap::from([(1, entity)]))
        .unwrap()
        .pop()
        .unwrap()
}

#[test]
fn held_break_repeats_action_and_swing_with_fresh_aim_without_catchup_bursts() {
    let mut app = app();
    let now = Instant::now();
    let cycle = Duration::from_secs_f32(crate::render::character_tool_duration(true));
    app.break_button(true, now);
    app.break_button(true, now);
    app.repeat_held_break(now + cycle - Duration::from_millis(1));
    assert_eq!(
        app.pending_commands.len(),
        1,
        "press acts immediately, once"
    );
    assert!(
        matches!(app.pending_commands.back(), Some(ClientMessage::Edit { x: 3, y: 41, z: 0, block, .. }) if *block == AIR)
    );
    let mut visual = avatar(&app);
    app.character_motion
        .apply(&mut visual, now + Duration::from_millis(200));
    assert!(
        matches!(visual.character_tool, Some((true, elapsed)) if (elapsed - 0.2).abs() < 0.001)
    );

    // A streamed replacement removes the first block and exposes the next one.
    let key = ChunkKey { x: 0, y: 2, z: 0 };
    let mut blocks = vec![AIR; crate::world::CHUNK_VOLUME];
    blocks[Chunk::index([4, 9, 0]).unwrap()] = crate::world::STONE;
    app.chunks
        .insert(key, Arc::new(Chunk::from_blocks(key, 2, blocks)));
    app.repeat_held_break(now + cycle);
    assert_eq!(app.pending_commands.len(), 2);
    assert!(
        matches!(app.pending_commands.back(), Some(ClientMessage::Edit { x: 4, y: 41, z: 0, block, .. }) if *block == AIR)
    );
    let ids: Vec<_> = app
        .pending_commands
        .iter()
        .filter_map(command_action_id)
        .collect();
    assert_ne!(ids[0], ids[1], "repeats are fresh actions, not retries");
    app.character_motion
        .apply(&mut visual, now + cycle + Duration::from_millis(100));
    assert!(
        matches!(visual.character_tool, Some((true, elapsed)) if (elapsed - 0.1).abs() < 0.001),
        "repeat restarts the swing"
    );

    app.repeat_held_break(now + Duration::from_secs(10));
    app.repeat_held_break(now + Duration::from_secs(10));
    assert_eq!(app.pending_commands.len(), 3, "stalled frames do not burst");
    app.break_button(false, now + Duration::from_secs(10));
    app.repeat_held_break(now + Duration::from_secs(20));
    assert_eq!(app.pending_commands.len(), 3, "release stops repeats");
}

#[test]
fn held_break_cancels_on_menus_capture_loss_and_session_retirement() {
    for stop in 0..3 {
        let mut app = app();
        let now = Instant::now();
        app.break_button(true, now);
        assert!(app.next_break.is_some());
        match stop {
            0 => app.set_screen(UiScreen::Pause),
            1 => app.set_grab(false),
            _ => app.retire_session(),
        }
        assert!(app.next_break.is_none());
        let queued = app.pending_commands.len();
        app.repeat_held_break(now + Duration::from_secs(2));
        assert_eq!(app.pending_commands.len(), queued);
        if !app.disconnected {
            app.set_screen(UiScreen::Playing);
            app.grabbed = true;
            app.repeat_held_break(now + Duration::from_secs(3));
            assert_eq!(
                app.pending_commands.len(),
                queued,
                "resume needs a new press"
            );
        }
    }
}

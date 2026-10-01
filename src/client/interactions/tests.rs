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
    let mut avatars = app
        .entity_registry
        .project(&BTreeMap::from([(1, entity)]))
        .unwrap();
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

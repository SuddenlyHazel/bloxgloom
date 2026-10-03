use super::*;

fn app() -> (ClientApp, std::sync::mpsc::Receiver<ClientMessage>) {
    let (network, outgoing) = Network::capture_outgoing_for_test();
    let mut app = ClientApp::new(
        network,
        Config::default(),
        std::env::temp_dir().join(format!(
            "controller-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        )),
    );
    app.grabbed = true;
    app.flight.flying = false;
    app.owned_entity_id = Some(1);
    (app, outgoing)
}
fn sample(app: &mut ClientApp, movement: Vec2, buttons: u16) {
    app.controller_sample(
        Snapshot {
            movement,
            buttons,
            ..Default::default()
        },
        0.01,
        Instant::now(),
    );
}

#[test]
fn radial_deadzone_keeps_analog_speed_and_bounds_diagonals_and_invalid_input() {
    assert_eq!(deadzone(Vec2::new(0.1, -0.1)), Vec2::ZERO);
    assert_eq!(deadzone(Vec2::new(f32::NAN, 0.0)), Vec2::ZERO);
    let half = deadzone(Vec2::new(0.0, 0.59));
    assert!((half.y - 0.5).abs() < 0.0001);
    assert!((deadzone(Vec2::ONE).length() - 1.0).abs() < 0.0001);
    assert!((look_delta(Vec2::X, 0.02) - 2.0 * look_delta(Vec2::X, 0.01)).length() < 0.0001);
}

#[test]
fn held_buttons_do_not_leak_across_screens_and_disconnect_releases_them() {
    let mut input = Input::default();
    let held = Snapshot {
        buttons: SOUTH | RT,
        movement: Vec2::Y,
        ..Default::default()
    };
    input.update(held);
    assert_eq!(input.pressed, SOUTH | RT);
    input.update(held);
    assert_eq!(input.pressed, 0);
    input.fence();
    input.update(held);
    assert_eq!(input.held, 0);
    input.update(Snapshot::default());
    assert_eq!(input.movement, Vec2::ZERO);
    input.update(held);
    assert_eq!(input.pressed, SOUTH | RT);
}

#[test]
fn controller_movement_and_sprint_use_authoritative_requests_and_preserve_analog_speed() {
    let (mut app, outgoing) = app();
    sample(&mut app, Vec2::new(0.0, 0.59), 0);
    app.move_player(0.01);
    let ClientMessage::Move { dx, dy, dz, .. } = outgoing.try_recv().unwrap() else {
        panic!("move expected");
    };
    let expected = app.catalog.player_rules().motion().intent_blocks_per_second * 0.005;
    assert!((Vec3::new(dx, dy, dz).length() - expected).abs() < 0.0001);
    sample(&mut app, Vec2::Y, L3);
    assert_eq!(
        outgoing.try_recv().unwrap(),
        ClientMessage::SetSprinting { sprinting: true }
    );
    assert!(!app.sprinting());
    app.accept_sprint(1, true);
    assert!(app.sprinting());
    sample(&mut app, Vec2::Y, L3);
    assert!(
        outgoing.try_recv().is_err(),
        "held L3 must not toggle every frame"
    );
    sample(&mut app, Vec2::ZERO, 0);
    assert_eq!(
        outgoing.try_recv().unwrap(),
        ClientMessage::SetSprinting { sprinting: false }
    );
    assert!(!app.sprinting());
}

#[test]
fn controller_jump_crouch_and_flight_follow_existing_gameplay_modes() {
    let (mut app, outgoing) = app();
    sample(&mut app, Vec2::ZERO, SOUTH);
    assert_eq!(outgoing.try_recv().unwrap(), ClientMessage::Jump);
    sample(&mut app, Vec2::ZERO, SOUTH);
    assert!(outgoing.try_recv().is_err());
    sample(&mut app, Vec2::ZERO, EAST);
    assert!(app.crouch_requested);
    assert!(!app.crouching(), "stance requires server approval");
    app.shift_down = true;
    sample(&mut app, Vec2::ZERO, 0);
    assert!(
        app.crouch_requested,
        "controller release must preserve keyboard crouch"
    );
    app.shift_down = false;
    app.flight.flying = true;
    sample(&mut app, Vec2::ZERO, SOUTH);
    assert!(app.controller.rising && !app.crouch_requested);
    sample(&mut app, Vec2::ZERO, EAST);
    assert!(app.controller.descending && !app.controller.crouching);
}

#[test]
fn mouse_and_controller_break_hold_are_combined_and_pause_clears_both() {
    let (mut app, _) = app();
    app.mouse_break_button(true, Instant::now());
    sample(&mut app, Vec2::ZERO, RT);
    app.mouse_break_button(false, Instant::now());
    assert!(app.next_break.is_some());
    sample(&mut app, Vec2::ZERO, 0);
    assert!(app.next_break.is_none());
    sample(&mut app, Vec2::Y, RT | START);
    assert_eq!(app.screen, UiScreen::Pause);
    assert!(app.next_break.is_none());
    assert_eq!(app.controller.movement, Vec2::ZERO);
    sample(&mut app, Vec2::Y, RT | START);
    assert_eq!(
        app.screen,
        UiScreen::Pause,
        "held Start must not reopen play"
    );
}

#[test]
fn menu_cursor_supports_drag_split_click_and_resizing_without_gameplay() {
    let (mut app, outgoing) = app();
    app.set_screen(UiScreen::Inventory);
    app.controller_sample(
        Snapshot {
            movement: Vec2::X,
            buttons: SOUTH,
            ..Default::default()
        },
        0.01,
        Instant::now(),
    );
    assert_eq!(app.controller.pointer_down, Some(false));
    assert!(app.cursor.0 > 640.0);
    sample(&mut app, Vec2::ZERO, WEST);
    assert_eq!(app.controller.pointer_down, Some(true));
    app.set_screen(UiScreen::Pause);
    assert_eq!(app.controller.pointer_down, None);
    sample(&mut app, Vec2::ZERO, WEST);
    assert_eq!(
        app.controller.pointer_down, None,
        "screen transition blocks held click"
    );
    sample(&mut app, Vec2::ZERO, 0);
    sample(&mut app, Vec2::ZERO, SOUTH);
    assert_eq!(app.controller.pointer_down, Some(false));
    assert!(outgoing.try_recv().is_err());
}

#[test]
fn unplugging_controller_cancels_menu_drag_and_preserves_mouse_and_keyboard_holds() {
    let (mut app, _) = app();
    app.mouse_break_button(true, Instant::now());
    app.shift_down = true;
    sample(&mut app, Vec2::Y, RT | EAST);
    app.controller_sample(
        Snapshot {
            unavailable: true,
            ..Default::default()
        },
        0.01,
        Instant::now(),
    );
    assert!(
        app.next_break.is_some(),
        "mouse break remains held after controller unplug"
    );
    assert!(app.crouch_requested, "keyboard Shift remains held");
    assert_eq!(app.controller.movement, Vec2::ZERO);
    app.set_screen(UiScreen::Inventory);
    sample(&mut app, Vec2::ZERO, SOUTH);
    assert_eq!(app.controller.pointer_down, Some(false));
    app.controller_sample(
        Snapshot {
            unavailable: true,
            ..Default::default()
        },
        0.01,
        Instant::now(),
    );
    assert_eq!(app.controller.pointer_down, None);
}

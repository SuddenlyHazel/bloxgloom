use super::*;
#[test]
fn transcript_deduplicates_server_ids_and_bounds_history() {
    let mut chat = Session::default();
    for id in 1..=80 {
        chat.received(bloxgloom_host_api::chat::Message {
            id,
            profile: 1,
            session: 1,
            name: "Rain".into(),
            text: format!("line {id}"),
        });
    }
    assert_eq!(chat.lines.len(), 64);
    assert_eq!(chat.lines.front().unwrap(), "Rain: line 17");
    chat.received(bloxgloom_host_api::chat::Message {
        id: 80,
        profile: 1,
        session: 1,
        name: "Rain".into(),
        text: "replayed".into(),
    });
    assert_eq!(chat.lines.back().unwrap(), "Rain: line 80");
    chat.history = VecDeque::from(["a".into(), "b".into()]);
    chat.history(true);
    assert_eq!(chat.input, "b");
    chat.history(true);
    assert_eq!(chat.input, "a");
    chat.history(false);
    assert_eq!(chat.input, "b");
    chat.history(false);
    assert!(chat.input.is_empty());
}
#[test]
fn chat_focus_clears_held_movement_and_sends_bounded_unicode() {
    let (network, outgoing) = Network::capture_outgoing_for_test();
    let mut app = ClientApp::new(
        network,
        Config::default(),
        std::env::temp_dir().join("unused-chat-config"),
    );
    app.screen = UiScreen::Playing;
    app.grabbed = true;
    app.keys.forward = true;
    app.shift_down = true;
    app.crouch_requested = true;
    assert!(app.chat_key(KeyCode::KeyT, Some("t"), true, false));
    assert!(app.chat.open);
    assert!(!app.keys.forward);
    assert!(!app.shift_down);
    assert!(!app.grabbed);
    assert!(app.chat_key(KeyCode::KeyW, None, false, false));
    assert!(!app.keys.forward);
    assert!(app.chat_key(KeyCode::KeyA, Some(&"é".repeat(300)), true, false));
    assert_eq!(app.chat.input.len(), 512);
    assert!(app.chat_key(KeyCode::Enter, None, true, false));
    assert!(!app.chat.open);
    let sent = outgoing
        .try_iter()
        .find_map(|message| {
            if let ClientMessage::Chat { sequence, text } = message {
                Some((sequence, text))
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(sent, (1, "é".repeat(256)));
    assert_eq!(app.chat.history.len(), 1);
    app.chat_key(KeyCode::KeyT, None, true, false);
    app.chat_key(KeyCode::ArrowUp, None, true, false);
    assert_eq!(app.chat.input.len(), 512);
    app.chat_key(KeyCode::Escape, None, true, false);
    assert!(!app.chat.open);
    assert!(app.chat.input.is_empty());
}

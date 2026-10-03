use super::*;
#[test]
fn unicode_chat_uses_bytes_and_rejects_blank_controls() {
    assert!(valid_text("héllo 雨 <3"));
    assert!(valid_text(&"x".repeat(MAX_TEXT_BYTES)));
    for text in [
        "".into(),
        "  ".into(),
        "line\nbreak".into(),
        "\0".into(),
        "é".repeat(MAX_TEXT_BYTES / 2 + 1),
    ] {
        assert!(!valid_text(&text));
    }
}

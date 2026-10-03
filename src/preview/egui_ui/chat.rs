//! Representative long messages for native chat layout verification.
pub(super) fn session(open: bool) -> crate::client::chat::Session {
    let mut chat = crate::client::chat::Session::default();
    for (name, text) in [
        (
            "Hazel",
            "Rain on these leaves sounds great now. <3".to_owned(),
        ),
        (
            "Builder",
            "雨 and café: Unicode reaches the server unchanged.".to_owned(),
        ),
        (
            "Explorer",
            "A long message tests readable wrapping and clipping. ".repeat(8),
        ),
        (
            "Hazel",
            "The spawn region grants exactly one reward per crossing.".to_owned(),
        ),
        (
            "Builder",
            "Let's test another look and a faster movement modifier.".to_owned(),
        ),
        (
            "Explorer",
            "Maximum-width input should keep the current caret visible.".to_owned(),
        ),
    ] {
        chat.notice(format!("{name}: {text}"));
    }
    chat.open = open;
    if open {
        chat.input = "Rain 雨 café <3 ".repeat(30);
        while chat.input.len() > bloxgloom_host_api::chat::MAX_TEXT_BYTES {
            chat.input.pop();
        }
    }
    chat
}

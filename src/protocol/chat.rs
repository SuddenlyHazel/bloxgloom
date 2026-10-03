//! Bounded UTF-8 chat frames and server-assigned sender/session identity.
use super::*;
use bloxgloom_host_api::chat::{Message, valid_text};
pub(super) fn write_text(out: &mut Vec<u8>, text: &str) -> io::Result<()> {
    if !valid_text(text) {
        return Err(invalid("invalid chat text"));
    }
    out.extend((text.len() as u16).to_le_bytes());
    out.extend(text.as_bytes());
    Ok(())
}
pub(super) fn read_text(cursor: &mut Cursor<'_>) -> io::Result<String> {
    let len = usize::from(cursor.u16()?);
    if len > bloxgloom_host_api::chat::MAX_TEXT_BYTES {
        return Err(invalid("chat text exceeds limit"));
    }
    let text = std::str::from_utf8(cursor.take(len)?)
        .map_err(|_| invalid("invalid chat UTF-8"))?
        .to_owned();
    if !valid_text(&text) {
        return Err(invalid("invalid chat text"));
    }
    Ok(text)
}
pub(super) fn write_message(out: &mut Vec<u8>, message: &Message) -> io::Result<()> {
    if message.id == 0
        || message.profile == 0
        || message.session == 0
        || message.name.is_empty()
        || message.name.chars().any(char::is_control)
    {
        return Err(invalid("invalid chat identity"));
    }
    out.extend(message.id.to_le_bytes());
    out.extend(message.profile.to_le_bytes());
    out.extend(message.session.to_le_bytes());
    short_string(out, &message.name)?;
    write_text(out, &message.text)
}
pub(super) fn read_message(cursor: &mut Cursor<'_>) -> io::Result<Message> {
    let message = Message {
        id: cursor.u64()?,
        profile: cursor.u128()?,
        session: cursor.u64()?,
        name: cursor.string()?,
        text: read_text(cursor)?,
    };
    if message.id == 0
        || message.profile == 0
        || message.session == 0
        || message.name.is_empty()
        || message.name.chars().any(char::is_control)
    {
        return Err(invalid("invalid chat identity"));
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chat_round_trip_preserves_exact_session_and_unicode_and_rejects_limits() {
        let message = Message {
            id: u64::MAX,
            profile: u128::MAX,
            session: u64::MAX,
            name: "Rain".into(),
            text: "雨 <3".into(),
        };
        let mut bytes = vec![WIRE_VERSION, 48];
        write_message(&mut bytes, &message).unwrap();
        assert_eq!(read_message(&mut Cursor::new(&bytes)).unwrap(), message);
        assert!(write_text(&mut Vec::new(), &"x".repeat(513)).is_err());
        assert!(write_text(&mut Vec::new(), "line\nbreak").is_err());
        assert!(read_text(&mut Cursor::new(&[WIRE_VERSION, 48, 1, 0, 0xff])).is_err());
        assert!(read_text(&mut Cursor::new(&[WIRE_VERSION, 48, 1, 2])).is_err());
    }
}

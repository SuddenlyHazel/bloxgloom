//! Console aliases and clock-face parsing. Permission is checked by the server.
use crate::daylight::CYCLE_MS;

pub(super) fn parse(arguments: &[&str]) -> Result<u64, &'static str> {
    let value = match arguments {
        [value] | ["set", value] => *value,
        _ => return Err("Usage: time set <sunrise|noon|sunset|midnight|HH:MM>"),
    };
    match value {
        "sunrise" | "dawn" => return Ok(0),
        "noon" | "day" => return Ok(CYCLE_MS / 4),
        "sunset" | "dusk" => return Ok(CYCLE_MS / 2),
        "midnight" | "night" => return Ok(CYCLE_MS * 3 / 4),
        _ => {}
    }
    let bytes = value.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || ![bytes[0], bytes[1], bytes[3], bytes[4]]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return Err("Use sunrise, noon, sunset, midnight, or HH:MM (00:00..23:59)");
    }
    let hours = u64::from((bytes[0] - b'0') * 10 + bytes[1] - b'0');
    let minutes = u64::from((bytes[3] - b'0') * 10 + bytes[4] - b'0');
    if hours >= 24 || minutes >= 60 {
        return Err("Clock time must be 00:00..23:59");
    }
    // Phase zero is sunrise at 06:00. The whole clock face spans twenty minutes.
    Ok(((hours * 60 + minutes + 18 * 60) % (24 * 60)) * CYCLE_MS / (24 * 60))
}
